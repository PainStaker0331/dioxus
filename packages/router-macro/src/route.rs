use quote::{__private::Span, format_ident, quote, ToTokens};
use syn::{Ident, LitStr, Type, Variant};
use syn::parse::ParseStream;
use syn::parse::Parse;

use proc_macro2::TokenStream as TokenStream2;

struct RouteArgs{
    route: LitStr,
    comp_name: Option<Ident>,
    props_name: Option<Ident>,
}

impl Parse for RouteArgs{
    fn parse(input: ParseStream<'_>) -> syn::Result<Self>{
        let route = input.parse::<LitStr>()?;

        Ok(RouteArgs {
            route,
            comp_name:input.parse().ok(),
            props_name: input.parse().ok(),
        })
    }
}

#[derive(Debug)]
pub struct Route {
    pub file_based: bool,
    pub route_name: Ident,
    pub comp_name: Ident,
    pub props_name: Ident,
    pub route: LitStr,
    pub route_segments: Vec<RouteSegment>,
}

impl Route {
    pub fn parse(input: syn::Variant) -> syn::Result<Self> {
        let route_attr = input
            .attrs
            .iter()
            .find(|attr| attr.path.is_ident("route"))
            .ok_or_else(|| {
                syn::Error::new_spanned(
                    input.clone(),
                    "Routable variants must have a #[route(...)] attribute",
                )
            })?;

        let route_name = input.ident.clone();
        let args = route_attr.parse_args::<RouteArgs>()?;
        let route = args.route;
        let file_based= args.comp_name.is_none();
        let comp_name = args.comp_name.unwrap_or_else(|| format_ident!("{}", route_name));
        let props_name = args.props_name.unwrap_or_else(|| format_ident!("{}Props", comp_name));

        let route_segments = parse_route_segments(&input, &route)?;

        Ok(Self {
            comp_name,
            props_name,
            route_name,
            route_segments,
            route,
            file_based,
        })
    }

    pub fn display_match(&self) -> TokenStream2 {
        let name = &self.route_name;
        let dynamic_segments = self.route_segments.iter().filter_map(|s| s.name());
        let write_segments = self.route_segments.iter().map(|s| s.write_segment());

        quote! {
            Self::#name { #(#dynamic_segments,)* } => {
                #(#write_segments)*
            }
        }
    }

    pub fn routable_match(&self) -> TokenStream2 {
        let name = &self.route_name;
        let dynamic_segments: Vec<_> = self
            .route_segments
            .iter()
            .filter_map(|s| s.name())
            .collect();
        let props_name = &self.props_name;
        let comp_name = &self.comp_name;

        quote! {
            Self::#name { #(#dynamic_segments,)* } => {
                let comp = #props_name { #(#dynamic_segments,)* };
                let cx = cx.bump().alloc(Scoped {
                    props: cx.bump().alloc(comp),
                    scope: cx,
                });
                #comp_name(cx)
            }
        }
    }

    pub fn construct(&self, enum_name: Ident) -> TokenStream2 {
        let segments = self.route_segments.iter().filter_map(|seg| {
            seg.name().map(|name| {
                quote! {
                    #name
                }
            })
        });
        let name = &self.route_name;

        quote! {
            #enum_name::#name {
                #(#segments,)*
            }
        }
    }

    pub fn error_ident(&self) -> Ident {
        format_ident!("{}ParseError", self.route_name)
    }

    pub fn error_type(&self) -> TokenStream2 {
        let error_name = self.error_ident();

        let mut error_variants = Vec::new();
        let mut display_match = Vec::new();

        for (i, segment) in self.route_segments.iter().enumerate() {
            let error_name = segment.error_name(i);
            match segment {
                RouteSegment::Static(index) => {
                    error_variants.push(quote! { #error_name });
                    display_match.push(quote! { Self::#error_name => write!(f, "Static segment '{}' did not match", #index)? });
                }
                RouteSegment::Dynamic(ident, ty) => {
                    error_variants.push(quote! { #error_name(<#ty as std::str::FromStr>::Err) });
                    display_match.push(quote! { Self::#error_name(err) => write!(f, "Dynamic segment '({}:{})' did not match: {}", stringify!(#ident), stringify!(#ty), err)? });
                }
                RouteSegment::CatchAll(ident, ty) => {
                    error_variants.push(quote! { #error_name(<#ty as std::str::FromStr>::Err) });
                    display_match.push(quote! { Self::#error_name(err) => write!(f, "Catch-all segment '({}:{})' did not match: {}", stringify!(#ident), stringify!(#ty), err)? });
                }
            }
        }

        quote! {
            #[allow(non_camel_case_types)]
            #[derive(Debug, PartialEq)]
            pub enum #error_name {
                ExtraSegments(String),
                #(#error_variants,)*
            }

            impl std::fmt::Display for #error_name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        Self::ExtraSegments(segments) => {
                            write!(f, "Found additional trailing segments: {segments}")?
                        }
                        #(#display_match,)*
                    }
                    Ok(())
                }
            }
        }
    }
}

impl ToTokens for Route {
    fn to_tokens(&self, tokens: &mut quote::__private::TokenStream) {
        if !self.file_based {
            return;
       }

        let without_leading_slash = &self.route.value()[1..];
        let route_path = std::path::Path::new(without_leading_slash);
        let with_extension = route_path.with_extension("rs");
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let dir = std::path::Path::new(&dir);
        let route = dir.join("src").join("pages").join(with_extension.clone());

        // check if the route exists or if not use the index route
        let route = if route.exists() && without_leading_slash != "" {
            with_extension.to_str().unwrap().to_string()
        } else {
            route_path.join("index.rs").to_str().unwrap().to_string()
        };

        let route_name: Ident = self.route_name.clone();
        let prop_name = &self.props_name;

        tokens.extend(quote!(
            #[path = #route]
            #[allow(non_snake_case)]
            mod #route_name;
            pub use #route_name::{#prop_name, #route_name};
        ));
    }
}

fn parse_route_segments(varient: &Variant, route: &LitStr) -> syn::Result<Vec<RouteSegment>> {
    let mut route_segments = Vec::new();

    let route_string = route.value();
    let mut iterator = route_string.split('/');

    // skip the first empty segment
    let first = iterator.next();
    if first != Some("") {
        return Err(syn::Error::new_spanned(
            varient,
            format!(
                "Routes should start with /. Error found in the route '{}'",
                route.value()
            ),
        ));
    }

    while let Some(segment) = iterator.next() {
        if segment.starts_with('(') && segment.ends_with(')') {
            let spread = segment.starts_with("(...");

            let ident = if spread {
                segment[3..segment.len() - 1].to_string()
            } else {
                segment[1..segment.len() - 1].to_string()
            };

            let field = varient.fields.iter().find(|field| match field.ident {
                Some(ref field_ident) => field_ident.to_string() == ident,
                None => false,
            });

            let ty = if let Some(field) = field {
                field.ty.clone()
            } else {
                return Err(syn::Error::new_spanned(
                    varient,
                    format!(
                        "Could not find a field with the name '{}' in the variant '{}'",
                        ident, varient.ident
                    ),
                ));
            };
            if spread {
                route_segments.push(RouteSegment::CatchAll(
                    Ident::new(&ident, Span::call_site()),
                    ty,
                ));

                if iterator.next().is_some() {
                    return Err(syn::Error::new_spanned(
                        route,
                        "Catch-all route segments must be the last segment in a route. The route segments after the catch-all segment will never be matched.",
                    ));
                } else {
                    break;
                }
            } else {
                route_segments.push(RouteSegment::Dynamic(
                    Ident::new(&ident, Span::call_site()),
                    ty,
                ));
            }
        } else {
            route_segments.push(RouteSegment::Static(segment.to_string()));
        }
    }

    Ok(route_segments)
}

#[derive(Debug)]
pub enum RouteSegment {
    Static(String),
    Dynamic(Ident, Type),
    CatchAll(Ident, Type),
}

impl RouteSegment {
    pub fn name(&self) -> Option<Ident> {
        match self {
            Self::Static(_) => None,
            Self::Dynamic(ident, _) => Some(ident.clone()),
            Self::CatchAll(ident, _) => Some(ident.clone()),
        }
    }

    pub fn write_segment(&self) -> TokenStream2 {
        match self {
            Self::Static(segment) => quote! { write!(f, "/{}", #segment)?; },
            Self::Dynamic(ident, _) => quote! { write!(f, "/{}", #ident)?; },
            Self::CatchAll(ident, _) => quote! { write!(f, "/{}", #ident)?; },
        }
    }

    fn error_name(&self, idx: usize) -> Ident {
        match self {
            Self::Static(_) => static_segment_idx(idx),
            Self::Dynamic(ident, _) => format_ident!("{}ParseError", ident),
            Self::CatchAll(ident, _) => format_ident!("{}ParseError", ident),
        }
    }

    pub fn try_parse(
        &self,
        idx: usize,
        error_enum_name: &Ident,
        error_enum_varient: &Ident,
        inner_parse_enum: &Ident,
    ) -> TokenStream2 {
        let error_name = self.error_name(idx);
        match self {
            Self::Static(segment) => {
                quote! {
                    let parsed = if segment == #segment {
                        Ok(())
                    } else {
                        Err(#error_enum_name::#error_enum_varient(#inner_parse_enum::#error_name))
                    };
                }
            }
            Self::Dynamic(_, ty) => {
                quote! {
                    let parsed = <#ty as std::str::FromStr>::from_str(segment).map_err(|err| #error_enum_name::#error_enum_varient(#inner_parse_enum::#error_name(err)));
                }
            }
            Self::CatchAll(_, _) => {
                todo!()
            }
        }
    }
}

pub fn static_segment_idx(idx: usize) -> Ident {
    format_ident!("StaticSegment{}ParseError", idx)
}
