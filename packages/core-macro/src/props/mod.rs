//! This code mostly comes from idanarye/rust-typed-builder
//!
//! However, it has been adopted to fit the Dioxus Props builder pattern.
//!
//! For Dioxus, we make a few changes:
//! - [ ] Automatically implement Into<Option> on the setters (IE the strip setter option)
//! - [ ] Automatically implement a default of none for optional fields (those explicitly wrapped with Option<T>)

use proc_macro2::TokenStream;

use syn::parse::Error;
use syn::spanned::Spanned;

use quote::quote;

pub fn impl_my_derive(ast: &syn::DeriveInput) -> Result<TokenStream, Error> {
    let data = match &ast.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => {
                let struct_info = struct_info::StructInfo::new(ast, fields.named.iter())?;
                let builder_creation = struct_info.builder_creation_impl()?;
                let conversion_helper = struct_info.conversion_helper_impl()?;
                let fields = struct_info
                    .included_fields()
                    .map(|f| struct_info.field_impl(f))
                    .collect::<Result<Vec<_>, _>>()?;
                let extends = struct_info
                    .extend_fields()
                    .map(|f| struct_info.extends_impl(f))
                    .collect::<Result<Vec<_>, _>>()?;
                let fields = quote!(#(#fields)*).into_iter();
                let required_fields = struct_info
                    .included_fields()
                    .filter(|f| f.builder_attr.default.is_none())
                    .map(|f| struct_info.required_field_impl(f))
                    .collect::<Result<Vec<_>, _>>()?;
                let build_method = struct_info.build_method_impl();

                quote! {
                    #builder_creation
                    #conversion_helper
                    #( #fields )*
                    #( #extends )*
                    #( #required_fields )*
                    #build_method
                }
            }
            syn::Fields::Unnamed(_) => {
                return Err(Error::new(
                    ast.span(),
                    "Props is not supported for tuple structs",
                ))
            }
            syn::Fields::Unit => {
                return Err(Error::new(
                    ast.span(),
                    "Props is not supported for unit structs",
                ))
            }
        },
        syn::Data::Enum(_) => {
            return Err(Error::new(ast.span(), "Props is not supported for enums"))
        }
        syn::Data::Union(_) => {
            return Err(Error::new(ast.span(), "Props is not supported for unions"))
        }
    };
    Ok(data)
}

mod util {
    use quote::ToTokens;

    pub fn path_to_single_string(path: &syn::Path) -> Option<String> {
        if path.leading_colon.is_some() {
            return None;
        }
        let mut it = path.segments.iter();
        let segment = it.next()?;
        if it.next().is_some() {
            // Multipart path
            return None;
        }
        if segment.arguments != syn::PathArguments::None {
            return None;
        }
        Some(segment.ident.to_string())
    }

    pub fn expr_to_single_string(expr: &syn::Expr) -> Option<String> {
        if let syn::Expr::Path(path) = expr {
            path_to_single_string(&path.path)
        } else {
            None
        }
    }

    pub fn ident_to_type(ident: syn::Ident) -> syn::Type {
        let mut path = syn::Path {
            leading_colon: None,
            segments: Default::default(),
        };
        path.segments.push(syn::PathSegment {
            ident,
            arguments: Default::default(),
        });
        syn::Type::Path(syn::TypePath { qself: None, path })
    }

    pub fn empty_type() -> syn::Type {
        syn::TypeTuple {
            paren_token: Default::default(),
            elems: Default::default(),
        }
        .into()
    }

    pub fn type_tuple(elems: impl Iterator<Item = syn::Type>) -> syn::TypeTuple {
        let mut result = syn::TypeTuple {
            paren_token: Default::default(),
            elems: elems.collect(),
        };
        if !result.elems.empty_or_trailing() {
            result.elems.push_punct(Default::default());
        }
        result
    }

    pub fn empty_type_tuple() -> syn::TypeTuple {
        syn::TypeTuple {
            paren_token: Default::default(),
            elems: Default::default(),
        }
    }

    pub fn make_punctuated_single<T, P: Default>(value: T) -> syn::punctuated::Punctuated<T, P> {
        let mut punctuated = syn::punctuated::Punctuated::new();
        punctuated.push(value);
        punctuated
    }

    pub fn modify_types_generics_hack<F>(
        ty_generics: &syn::TypeGenerics,
        mut mutator: F,
    ) -> syn::AngleBracketedGenericArguments
    where
        F: FnMut(&mut syn::punctuated::Punctuated<syn::GenericArgument, syn::token::Comma>),
    {
        let mut abga: syn::AngleBracketedGenericArguments =
            syn::parse(ty_generics.clone().into_token_stream().into()).unwrap_or_else(|_| {
                syn::AngleBracketedGenericArguments {
                    colon2_token: None,
                    lt_token: Default::default(),
                    args: Default::default(),
                    gt_token: Default::default(),
                }
            });
        mutator(&mut abga.args);
        abga
    }

    pub fn strip_raw_ident_prefix(mut name: String) -> String {
        if name.starts_with("r#") {
            name.replace_range(0..2, "");
        }
        name
    }
}

mod field_info {
    use crate::props::type_from_inside_option;
    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::spanned::Spanned;
    use syn::{parse::Error, punctuated::Punctuated};
    use syn::{Expr, Path};

    use super::util::{
        expr_to_single_string, ident_to_type, path_to_single_string, strip_raw_ident_prefix,
    };

    #[derive(Debug)]
    pub struct FieldInfo<'a> {
        pub ordinal: usize,
        pub name: &'a syn::Ident,
        pub generic_ident: syn::Ident,
        pub ty: &'a syn::Type,
        pub builder_attr: FieldBuilderAttr,
    }

    impl<'a> FieldInfo<'a> {
        pub fn new(
            ordinal: usize,
            field: &syn::Field,
            field_defaults: FieldBuilderAttr,
        ) -> Result<FieldInfo, Error> {
            if let Some(ref name) = field.ident {
                let mut builder_attr = field_defaults.with(&field.attrs)?;

                // children field is automatically defaulted to None
                if name == "children" {
                    builder_attr.default = Some(
                        syn::parse(quote!(::core::default::Default::default()).into()).unwrap(),
                    );
                }

                // extended field is automatically empty
                if !builder_attr.extends.is_empty() {
                    builder_attr.default = Some(
                        syn::parse(quote!(::core::default::Default::default()).into()).unwrap(),
                    );
                }

                // auto detect optional
                let strip_option_auto = builder_attr.strip_option
                    || !builder_attr.ignore_option
                        && type_from_inside_option(&field.ty, true).is_some();
                if !builder_attr.strip_option && strip_option_auto {
                    builder_attr.strip_option = true;
                    builder_attr.default = Some(
                        syn::parse(quote!(::core::default::Default::default()).into()).unwrap(),
                    );
                }

                Ok(FieldInfo {
                    ordinal,
                    name,
                    generic_ident: syn::Ident::new(
                        &format!("__{}", strip_raw_ident_prefix(name.to_string())),
                        name.span(),
                    ),
                    ty: &field.ty,
                    builder_attr,
                })
            } else {
                Err(Error::new(field.span(), "Nameless field in struct"))
            }
        }

        pub fn generic_ty_param(&self) -> syn::GenericParam {
            syn::GenericParam::Type(self.generic_ident.clone().into())
        }

        pub fn type_ident(&self) -> syn::Type {
            ident_to_type(self.generic_ident.clone())
        }

        pub fn tuplized_type_ty_param(&self) -> syn::Type {
            let mut types = syn::punctuated::Punctuated::default();
            types.push(self.ty.clone());
            types.push_punct(Default::default());
            syn::TypeTuple {
                paren_token: Default::default(),
                elems: types,
            }
            .into()
        }

        pub fn type_from_inside_option(&self, check_option_name: bool) -> Option<&syn::Type> {
            type_from_inside_option(self.ty, check_option_name)
        }
    }

    #[derive(Debug, Default, Clone)]
    pub struct FieldBuilderAttr {
        pub default: Option<syn::Expr>,
        pub doc: Option<syn::Expr>,
        pub skip: bool,
        pub auto_into: bool,
        pub strip_option: bool,
        pub ignore_option: bool,
        pub extends: Vec<Path>,
    }

    impl FieldBuilderAttr {
        pub fn with(mut self, attrs: &[syn::Attribute]) -> Result<Self, Error> {
            let mut skip_tokens = None;
            for attr in attrs {
                if path_to_single_string(attr.path()).as_deref() != Some("props") {
                    continue;
                }

                match &attr.meta {
                    syn::Meta::List(list) => {
                        if list.tokens.is_empty() {
                            continue;
                        }
                    }
                    _ => {
                        continue;
                    }
                }

                let as_expr = attr.parse_args_with(
                    Punctuated::<Expr, syn::Token![,]>::parse_separated_nonempty,
                )?;

                for expr in as_expr.into_iter() {
                    self.apply_meta(expr)?;
                }

                // Stash its span for later (we don’t yet know if it’ll be an error)
                if self.skip && skip_tokens.is_none() {
                    skip_tokens = Some(attr.meta.clone());
                }
            }

            if self.skip && self.default.is_none() {
                return Err(Error::new_spanned(
                    skip_tokens.unwrap(),
                    "#[props(skip)] must be accompanied by default or default_code",
                ));
            }

            Ok(self)
        }

        pub fn apply_meta(&mut self, expr: syn::Expr) -> Result<(), Error> {
            match expr {
                // #[props(default = "...")]
                syn::Expr::Assign(assign) => {
                    let name = expr_to_single_string(&assign.left)
                        .ok_or_else(|| Error::new_spanned(&assign.left, "Expected identifier"))?;
                    match name.as_str() {
                        "extends" => {
                            if let syn::Expr::Path(path) = *assign.right {
                                self.extends.push(path.path);
                                Ok(())
                            } else {
                                Err(Error::new_spanned(
                                    assign.right,
                                    "Expected simple identifier",
                                ))
                            }
                        }
                        "default" => {
                            self.default = Some(*assign.right);
                            Ok(())
                        }
                        "doc" => {
                            self.doc = Some(*assign.right);
                            Ok(())
                        }
                        "default_code" => {
                            if let syn::Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Str(code),
                                ..
                            }) = *assign.right
                            {
                                use std::str::FromStr;
                                let tokenized_code = TokenStream::from_str(&code.value())?;
                                self.default = Some(
                                    syn::parse(tokenized_code.into())
                                        .map_err(|e| Error::new_spanned(code, format!("{e}")))?,
                                );
                            } else {
                                return Err(Error::new_spanned(assign.right, "Expected string"));
                            }
                            Ok(())
                        }
                        _ => Err(Error::new_spanned(
                            &assign,
                            format!("Unknown parameter {name:?}"),
                        )),
                    }
                }

                // #[props(default)]
                syn::Expr::Path(path) => {
                    let name = path_to_single_string(&path.path)
                        .ok_or_else(|| Error::new_spanned(&path, "Expected identifier"))?;
                    match name.as_str() {
                        "default" => {
                            self.default = Some(
                                syn::parse(quote!(::core::default::Default::default()).into())
                                    .unwrap(),
                            );
                            Ok(())
                        }

                        "optional" => {
                            self.default = Some(
                                syn::parse(quote!(::core::default::Default::default()).into())
                                    .unwrap(),
                            );
                            self.strip_option = true;
                            Ok(())
                        }

                        "extend" => {
                            self.extends.push(path.path);
                            Ok(())
                        }

                        _ => {
                            macro_rules! handle_fields {
                                ( $( $flag:expr, $field:ident, $already:expr; )* ) => {
                                    match name.as_str() {
                                        $(
                                            $flag => {
                                                if self.$field {
                                                    Err(Error::new(path.span(), concat!("Illegal setting - field is already ", $already)))
                                                } else {
                                                    self.$field = true;
                                                    Ok(())
                                                }
                                            }
                                        )*
                                        _ => Err(Error::new_spanned(
                                                &path,
                                                format!("Unknown setter parameter {:?}", name),
                                        ))
                                    }
                                }
                            }
                            handle_fields!(
                                "skip", skip, "skipped";
                                "into", auto_into, "calling into() on the argument";
                                "strip_option", strip_option, "putting the argument in Some(...)";
                            )
                        }
                    }
                }

                syn::Expr::Unary(syn::ExprUnary {
                    op: syn::UnOp::Not(_),
                    expr,
                    ..
                }) => {
                    if let syn::Expr::Path(path) = *expr {
                        let name = path_to_single_string(&path.path)
                            .ok_or_else(|| Error::new_spanned(&path, "Expected identifier"))?;
                        match name.as_str() {
                            "default" => {
                                self.default = None;
                                Ok(())
                            }
                            "doc" => {
                                self.doc = None;
                                Ok(())
                            }
                            "skip" => {
                                self.skip = false;
                                Ok(())
                            }
                            "auto_into" => {
                                self.auto_into = false;
                                Ok(())
                            }
                            "optional" => {
                                self.strip_option = false;
                                self.ignore_option = true;
                                Ok(())
                            }
                            _ => Err(Error::new_spanned(path, "Unknown setting".to_owned())),
                        }
                    } else {
                        Err(Error::new_spanned(
                            expr,
                            "Expected simple identifier".to_owned(),
                        ))
                    }
                }
                _ => Err(Error::new_spanned(expr, "Expected (<...>=<...>)")),
            }
        }
    }
}

fn type_from_inside_option(ty: &syn::Type, check_option_name: bool) -> Option<&syn::Type> {
    let path = if let syn::Type::Path(type_path) = ty {
        if type_path.qself.is_some() {
            return None;
        } else {
            &type_path.path
        }
    } else {
        return None;
    };
    let segment = path.segments.last()?;
    if check_option_name && segment.ident != "Option" {
        return None;
    }
    let generic_params =
        if let syn::PathArguments::AngleBracketed(generic_params) = &segment.arguments {
            generic_params
        } else {
            return None;
        };
    if let syn::GenericArgument::Type(ty) = generic_params.args.first()? {
        Some(ty)
    } else {
        None
    }
}

mod struct_info {
    use convert_case::{Case, Casing};
    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::parse::Error;
    use syn::punctuated::Punctuated;
    use syn::spanned::Spanned;
    use syn::visit::Visit;
    use syn::{parse_quote, Expr, Ident};

    use super::field_info::{FieldBuilderAttr, FieldInfo};
    use super::util::{
        empty_type, empty_type_tuple, expr_to_single_string, make_punctuated_single,
        modify_types_generics_hack, path_to_single_string, strip_raw_ident_prefix, type_tuple,
    };

    #[derive(Debug)]
    pub struct StructInfo<'a> {
        pub vis: &'a syn::Visibility,
        pub name: &'a syn::Ident,
        pub generics: &'a syn::Generics,
        pub fields: Vec<FieldInfo<'a>>,

        pub builder_attr: TypeBuilderAttr,
        pub builder_name: syn::Ident,
        pub conversion_helper_trait_name: syn::Ident,
        pub core: syn::Ident,
    }

    impl<'a> StructInfo<'a> {
        pub fn included_fields(&self) -> impl Iterator<Item = &FieldInfo<'a>> {
            self.fields
                .iter()
                .filter(|f| !f.builder_attr.skip && f.builder_attr.extends.is_empty())
        }

        pub fn extend_fields(&self) -> impl Iterator<Item = &FieldInfo<'a>> {
            self.fields
                .iter()
                .filter(|f| !f.builder_attr.extends.is_empty())
        }

        fn extend_lifetime(&self) -> syn::Result<Option<syn::Lifetime>> {
            let first_extend = self.extend_fields().next();

            match first_extend {
                Some(f) => {
                    struct VisitFirstLifetime(Option<syn::Lifetime>);

                    impl Visit<'_> for VisitFirstLifetime {
                        fn visit_lifetime(&mut self, lifetime: &'_ syn::Lifetime) {
                            if self.0.is_none() {
                                self.0 = Some(lifetime.clone());
                            }
                        }
                    }

                    let name = f.name;
                    let mut visitor = VisitFirstLifetime(None);

                    visitor.visit_type(f.ty);

                    visitor.0.ok_or_else(|| {
                        syn::Error::new_spanned(
                            name,
                            "Unable to find lifetime for extended field. Please specify it manually",
                        )
                    }).map(Some)
                }
                None => Ok(None),
            }
        }

        pub fn new(
            ast: &'a syn::DeriveInput,
            fields: impl Iterator<Item = &'a syn::Field>,
        ) -> Result<StructInfo<'a>, Error> {
            let builder_attr = TypeBuilderAttr::new(&ast.attrs)?;
            let builder_name = strip_raw_ident_prefix(format!("{}Builder", ast.ident));
            Ok(StructInfo {
                vis: &ast.vis,
                name: &ast.ident,
                generics: &ast.generics,
                fields: fields
                    .enumerate()
                    .map(|(i, f)| FieldInfo::new(i, f, builder_attr.field_defaults.clone()))
                    .collect::<Result<_, _>>()?,
                builder_attr,
                builder_name: syn::Ident::new(&builder_name, ast.ident.span()),
                conversion_helper_trait_name: syn::Ident::new(
                    &format!("{builder_name}_Optional"),
                    ast.ident.span(),
                ),
                core: syn::Ident::new(&format!("{builder_name}_core"), ast.ident.span()),
            })
        }

        fn modify_generics<F: FnMut(&mut syn::Generics)>(&self, mut mutator: F) -> syn::Generics {
            let mut generics = self.generics.clone();
            mutator(&mut generics);
            generics
        }

        pub fn builder_creation_impl(&self) -> Result<TokenStream, Error> {
            let StructInfo {
                ref vis,
                ref name,
                ref builder_name,
                ..
            } = *self;

            // we're generating stuff that goes into unsafe code here
            // we use the heuristic: are there *any* generic parameters?
            // If so, then they might have non-static lifetimes and we can't compare two generic things that *might borrow*
            // Therefore, we will generate code that shortcircuits the "comparison" in memoization
            let are_there_generics = !self.generics.params.is_empty();

            let extend_lifetime = self.extend_lifetime()?;

            let generics = self.generics.clone();
            let (_, ty_generics, where_clause) = generics.split_for_impl();
            let impl_generics = self.modify_generics(|g| {
                if extend_lifetime.is_none() {
                    g.params.insert(0, parse_quote!('__bump));
                }
            });
            let (impl_generics, _, _) = impl_generics.split_for_impl();
            let (_, b_initial_generics, _) = self.generics.split_for_impl();
            let all_fields_param = syn::GenericParam::Type(
                syn::Ident::new("TypedBuilderFields", proc_macro2::Span::call_site()).into(),
            );
            let b_generics = self.modify_generics(|g| {
                g.params.insert(0, all_fields_param.clone());
            });
            let empties_tuple = type_tuple(self.included_fields().map(|_| empty_type()));
            let generics_with_empty = modify_types_generics_hack(&b_initial_generics, |args| {
                args.insert(0, syn::GenericArgument::Type(empties_tuple.clone().into()));
            });
            let phantom_generics = self.generics.params.iter().map(|param| match param {
                syn::GenericParam::Lifetime(lifetime) => {
                    let lifetime = &lifetime.lifetime;
                    quote!(::core::marker::PhantomData<&#lifetime ()>)
                }
                syn::GenericParam::Type(ty) => {
                    let ty = &ty.ident;
                    quote!(::core::marker::PhantomData<#ty>)
                }
                syn::GenericParam::Const(_cnst) => {
                    quote!()
                }
            });
            let builder_method_doc = match self.builder_attr.builder_method_doc {
                Some(ref doc) => quote!(#doc),
                None => {
                    let doc = format!(
                        "
Create a builder for building `{name}`.
On the builder, call {setters} to set the values of the fields.
Finally, call `.build()` to create the instance of `{name}`.
                    ",
                        name = self.name,
                        setters = {
                            let mut result = String::new();
                            let mut is_first = true;
                            for field in self.included_fields() {
                                use std::fmt::Write;
                                if is_first {
                                    is_first = false;
                                } else {
                                    write!(&mut result, ", ").unwrap();
                                }
                                write!(&mut result, "`.{}(...)`", field.name).unwrap();
                                if field.builder_attr.default.is_some() {
                                    write!(&mut result, "(optional)").unwrap();
                                }
                            }
                            result
                        }
                    );
                    quote!(#doc)
                }
            };
            let builder_type_doc = if self.builder_attr.doc {
                match self.builder_attr.builder_type_doc {
                    Some(ref doc) => quote!(#[doc = #doc]),
                    None => {
                        let doc = format!(
                        "Builder for [`{name}`] instances.\n\nSee [`{name}::builder()`] for more info.",
                    );
                        quote!(#[doc = #doc])
                    }
                }
            } else {
                quote!(#[doc(hidden)])
            };

            let (_, _, b_generics_where_extras_predicates) = b_generics.split_for_impl();
            let mut b_generics_where: syn::WhereClause = syn::parse2(quote! {
                where TypedBuilderFields: Clone
            })?;
            if let Some(predicates) = b_generics_where_extras_predicates {
                b_generics_where
                    .predicates
                    .extend(predicates.predicates.clone());
            }

            let can_memoize = match are_there_generics {
                true => quote! { false  },
                false => quote! { self == other },
            };

            let is_static = match are_there_generics {
                true => quote! { false  },
                false => quote! { true },
            };

            let extend_fields = self.extend_fields().map(|f| {
                let name = f.name;
                let ty = f.ty;
                quote!(#name: #ty)
            });
            let extend_fields_value = self.extend_fields().map(|f| {
                let name = f.name;
                quote!(#name: Vec::new())
            });
            let has_extend_fields = self.extend_fields().next().is_some();
            let take_bump = if has_extend_fields {
                quote!(bump: _cx.bump(),)
            } else {
                quote!()
            };
            let bump_field = if has_extend_fields {
                quote!(bump: & #extend_lifetime ::dioxus::core::exports::bumpalo::Bump,)
            } else {
                quote!()
            };
            let extend_lifetime = extend_lifetime.unwrap_or(syn::Lifetime::new(
                "'__bump",
                proc_macro2::Span::call_site(),
            ));

            Ok(quote! {
                impl #impl_generics #name #ty_generics #where_clause {
                    #[doc = #builder_method_doc]
                    #[allow(dead_code)]
                    #vis fn builder(_cx: & #extend_lifetime ::dioxus::prelude::ScopeState) -> #builder_name #generics_with_empty {
                        #builder_name {
                            #(#extend_fields_value,)*
                            #take_bump
                            fields: #empties_tuple,
                            _phantom: ::core::default::Default::default(),
                        }
                    }
                }

                #[must_use]
                #builder_type_doc
                #[allow(dead_code, non_camel_case_types, non_snake_case)]
                #vis struct #builder_name #b_generics {
                    #(#extend_fields,)*
                    #bump_field
                    fields: #all_fields_param,
                    _phantom: (#( #phantom_generics ),*),
                }

                impl #impl_generics ::dioxus::prelude::Properties<#extend_lifetime> for #name #ty_generics
                #b_generics_where_extras_predicates
                {
                    type Builder = #builder_name #generics_with_empty;
                    const IS_STATIC: bool = #is_static;
                    fn builder(_cx: &#extend_lifetime ::dioxus::prelude::ScopeState) -> Self::Builder {
                        #name::builder(_cx)
                    }
                    unsafe fn memoize(&self, other: &Self) -> bool {
                        #can_memoize
                    }
                }

            })
        }

        // TODO: once the proc-macro crate limitation is lifted, make this an util trait of this
        // crate.
        pub fn conversion_helper_impl(&self) -> Result<TokenStream, Error> {
            let trait_name = &self.conversion_helper_trait_name;
            Ok(quote! {
                #[doc(hidden)]
                #[allow(dead_code, non_camel_case_types, non_snake_case)]
                pub trait #trait_name<T> {
                    fn into_value<F: FnOnce() -> T>(self, default: F) -> T;
                }

                impl<T> #trait_name<T> for () {
                    fn into_value<F: FnOnce() -> T>(self, default: F) -> T {
                        default()
                    }
                }

                impl<T> #trait_name<T> for (T,) {
                    fn into_value<F: FnOnce() -> T>(self, _: F) -> T {
                        self.0
                    }
                }
            })
        }

        pub fn extends_impl(&self, field: &FieldInfo) -> Result<TokenStream, Error> {
            let StructInfo {
                ref builder_name, ..
            } = *self;

            let field_name = field.name;

            let descructuring = self.included_fields().map(|f| {
                if f.ordinal == field.ordinal {
                    quote!(_)
                } else {
                    let name = f.name;
                    quote!(#name)
                }
            });
            let reconstructing = self.included_fields().map(|f| f.name);

            // Add the bump lifetime to the generics
            let mut ty_generics: Vec<syn::GenericArgument> = self
                .generics
                .params
                .iter()
                .map(|generic_param| match generic_param {
                    syn::GenericParam::Type(type_param) => {
                        let ident = type_param.ident.clone();
                        syn::parse(quote!(#ident).into()).unwrap()
                    }
                    syn::GenericParam::Lifetime(lifetime_def) => {
                        syn::GenericArgument::Lifetime(lifetime_def.lifetime.clone())
                    }
                    syn::GenericParam::Const(const_param) => {
                        let ident = const_param.ident.clone();
                        syn::parse(quote!(#ident).into()).unwrap()
                    }
                })
                .collect();
            let mut target_generics_tuple = empty_type_tuple();
            let mut ty_generics_tuple = empty_type_tuple();
            let generics = self.modify_generics(|g| {
                let index_after_lifetime_in_generics = g
                    .params
                    .iter()
                    .filter(|arg| matches!(arg, syn::GenericParam::Lifetime(_)))
                    .count();
                for f in self.included_fields() {
                    if f.ordinal == field.ordinal {
                        ty_generics_tuple.elems.push_value(empty_type());
                        target_generics_tuple
                            .elems
                            .push_value(f.tuplized_type_ty_param());
                    } else {
                        g.params
                            .insert(index_after_lifetime_in_generics, f.generic_ty_param());
                        let generic_argument: syn::Type = f.type_ident();
                        ty_generics_tuple.elems.push_value(generic_argument.clone());
                        target_generics_tuple.elems.push_value(generic_argument);
                    }
                    ty_generics_tuple.elems.push_punct(Default::default());
                    target_generics_tuple.elems.push_punct(Default::default());
                }
            });
            let mut target_generics = ty_generics.clone();
            let index_after_lifetime_in_generics = target_generics
                .iter()
                .filter(|arg| matches!(arg, syn::GenericArgument::Lifetime(_)))
                .count();
            target_generics.insert(
                index_after_lifetime_in_generics,
                syn::GenericArgument::Type(target_generics_tuple.into()),
            );
            ty_generics.insert(
                index_after_lifetime_in_generics,
                syn::GenericArgument::Type(ty_generics_tuple.into()),
            );
            let (impl_generics, _, where_clause) = generics.split_for_impl();

            let forward_extended_fields = self.extend_fields().map(|f| {
                let name = f.name;
                quote!(#name: self.#name)
            });

            let extend_lifetime = self.extend_lifetime()?.ok_or(Error::new_spanned(
                field_name,
                "Unable to find lifetime for extended field. Please specify it manually",
            ))?;

            let extends_impl = field.builder_attr.extends.iter().map(|path| {
                let name_str = path_to_single_string(path).unwrap();
                let camel_name = name_str.to_case(Case::UpperCamel);
                let marker_name = Ident::new(
                    format!("{}Extension", &camel_name).as_str(),
                    path.span(),
                );
                quote! {
                    #[allow(dead_code, non_camel_case_types, missing_docs)]
                    impl #impl_generics dioxus_elements::extensions::#marker_name < #extend_lifetime > for #builder_name < #( #ty_generics ),* > #where_clause {}
                }
            });

            Ok(quote! {
                #[allow(dead_code, non_camel_case_types, missing_docs)]
                impl #impl_generics ::dioxus::prelude::HasAttributes<#extend_lifetime> for #builder_name < #( #ty_generics ),* > #where_clause {
                    fn push_attribute(
                        mut self,
                        name: &#extend_lifetime str,
                        ns: Option<&'static str>,
                        attr: impl ::dioxus::prelude::IntoAttributeValue<#extend_lifetime>,
                        volatile: bool
                    ) -> Self {
                        let ( #(#descructuring,)* ) = self.fields;
                        self.#field_name.push(
                            ::dioxus::core::Attribute::new(
                                name,
                                {
                                    use ::dioxus::prelude::IntoAttributeValue;
                                    attr.into_value(self.bump)
                                },
                                ns,
                                volatile,
                            )
                        );
                        #builder_name {
                            #(#forward_extended_fields,)*
                            bump: self.bump,
                            fields: ( #(#reconstructing,)* ),
                            _phantom: self._phantom,
                        }
                    }
                }

                #(#extends_impl)*
            })
        }

        pub fn field_impl(&self, field: &FieldInfo) -> Result<TokenStream, Error> {
            let StructInfo {
                ref builder_name, ..
            } = *self;

            let descructuring = self.included_fields().map(|f| {
                if f.ordinal == field.ordinal {
                    quote!(_)
                } else {
                    let name = f.name;
                    quote!(#name)
                }
            });
            let reconstructing = self.included_fields().map(|f| f.name);

            let FieldInfo {
                name: field_name,
                ty: field_type,
                ..
            } = field;
            // Add the bump lifetime to the generics
            let mut ty_generics: Vec<syn::GenericArgument> = self
                .generics
                .params
                .iter()
                .map(|generic_param| match generic_param {
                    syn::GenericParam::Type(type_param) => {
                        let ident = type_param.ident.clone();
                        syn::parse(quote!(#ident).into()).unwrap()
                    }
                    syn::GenericParam::Lifetime(lifetime_def) => {
                        syn::GenericArgument::Lifetime(lifetime_def.lifetime.clone())
                    }
                    syn::GenericParam::Const(const_param) => {
                        let ident = const_param.ident.clone();
                        syn::parse(quote!(#ident).into()).unwrap()
                    }
                })
                .collect();
            let mut target_generics_tuple = empty_type_tuple();
            let mut ty_generics_tuple = empty_type_tuple();
            let generics = self.modify_generics(|g| {
                let index_after_lifetime_in_generics = g
                    .params
                    .iter()
                    .filter(|arg| matches!(arg, syn::GenericParam::Lifetime(_)))
                    .count();
                for f in self.included_fields() {
                    if f.ordinal == field.ordinal {
                        ty_generics_tuple.elems.push_value(empty_type());
                        target_generics_tuple
                            .elems
                            .push_value(f.tuplized_type_ty_param());
                    } else {
                        g.params
                            .insert(index_after_lifetime_in_generics, f.generic_ty_param());
                        let generic_argument: syn::Type = f.type_ident();
                        ty_generics_tuple.elems.push_value(generic_argument.clone());
                        target_generics_tuple.elems.push_value(generic_argument);
                    }
                    ty_generics_tuple.elems.push_punct(Default::default());
                    target_generics_tuple.elems.push_punct(Default::default());
                }
            });
            let mut target_generics = ty_generics.clone();
            let index_after_lifetime_in_generics = target_generics
                .iter()
                .filter(|arg| matches!(arg, syn::GenericArgument::Lifetime(_)))
                .count();
            target_generics.insert(
                index_after_lifetime_in_generics,
                syn::GenericArgument::Type(target_generics_tuple.into()),
            );
            ty_generics.insert(
                index_after_lifetime_in_generics,
                syn::GenericArgument::Type(ty_generics_tuple.into()),
            );
            let (impl_generics, _, where_clause) = generics.split_for_impl();
            let doc = match field.builder_attr.doc {
                Some(ref doc) => quote!(#[doc = #doc]),
                None => quote!(),
            };

            // NOTE: both auto_into and strip_option affect `arg_type` and `arg_expr`, but the order of
            // nesting is different so we have to do this little dance.
            let arg_type = if field.builder_attr.strip_option {
                field.type_from_inside_option(false).ok_or_else(|| {
                    Error::new_spanned(
                        field_type,
                        "can't `strip_option` - field is not `Option<...>`",
                    )
                })?
            } else {
                field_type
            };
            let (arg_type, arg_expr) = if field.builder_attr.auto_into {
                (
                    quote!(impl ::core::convert::Into<#arg_type>),
                    quote!(#field_name.into()),
                )
            } else {
                (quote!(#arg_type), quote!(#field_name))
            };
            let arg_expr = if field.builder_attr.strip_option {
                quote!(Some(#arg_expr))
            } else {
                arg_expr
            };

            let repeated_fields_error_type_name = syn::Ident::new(
                &format!(
                    "{}_Error_Repeated_field_{}",
                    builder_name,
                    strip_raw_ident_prefix(field_name.to_string())
                ),
                builder_name.span(),
            );
            let repeated_fields_error_message = format!("Repeated field {field_name}");

            let forward_extended_fields = self.extend_fields().map(|f| {
                let name = f.name;
                quote!(#name: self.#name)
            });
            let forward_bump = if self.extend_fields().next().is_some() {
                quote!(bump: self.bump,)
            } else {
                quote!()
            };

            Ok(quote! {
                #[allow(dead_code, non_camel_case_types, missing_docs)]
                impl #impl_generics #builder_name < #( #ty_generics ),* > #where_clause {
                    #doc
                    pub fn #field_name (self, #field_name: #arg_type) -> #builder_name < #( #target_generics ),* > {
                        let #field_name = (#arg_expr,);
                        let ( #(#descructuring,)* ) = self.fields;
                        #builder_name {
                            #(#forward_extended_fields,)*
                            #forward_bump
                            fields: ( #(#reconstructing,)* ),
                            _phantom: self._phantom,
                        }
                    }
                }
                #[doc(hidden)]
                #[allow(dead_code, non_camel_case_types, non_snake_case)]
                pub enum #repeated_fields_error_type_name {}
                #[doc(hidden)]
                #[allow(dead_code, non_camel_case_types, missing_docs)]
                impl #impl_generics #builder_name < #( #target_generics ),* > #where_clause {
                    #[deprecated(
                        note = #repeated_fields_error_message
                    )]
                    pub fn #field_name (self, _: #repeated_fields_error_type_name) -> #builder_name < #( #target_generics ),* > {
                        self
                    }
                }
            })
        }

        pub fn required_field_impl(&self, field: &FieldInfo) -> Result<TokenStream, Error> {
            let StructInfo {
                ref name,
                ref builder_name,
                ..
            } = self;

            let FieldInfo {
                name: ref field_name,
                ..
            } = field;
            // Add a bump lifetime to the generics
            let mut builder_generics: Vec<syn::GenericArgument> = self
                .generics
                .params
                .iter()
                .map(|generic_param| match generic_param {
                    syn::GenericParam::Type(type_param) => {
                        let ident = &type_param.ident;
                        syn::parse(quote!(#ident).into()).unwrap()
                    }
                    syn::GenericParam::Lifetime(lifetime_def) => {
                        syn::GenericArgument::Lifetime(lifetime_def.lifetime.clone())
                    }
                    syn::GenericParam::Const(const_param) => {
                        let ident = &const_param.ident;
                        syn::parse(quote!(#ident).into()).unwrap()
                    }
                })
                .collect();
            let mut builder_generics_tuple = empty_type_tuple();
            let generics = self.modify_generics(|g| {
                let index_after_lifetime_in_generics = g
                    .params
                    .iter()
                    .filter(|arg| matches!(arg, syn::GenericParam::Lifetime(_)))
                    .count();
                for f in self.included_fields() {
                    if f.builder_attr.default.is_some() {
                        // `f` is not mandatory - it does not have it's own fake `build` method, so `field` will need
                        // to warn about missing `field` whether or not `f` is set.
                        assert!(
                            f.ordinal != field.ordinal,
                            "`required_field_impl` called for optional field {}",
                            field.name
                        );
                        g.params
                            .insert(index_after_lifetime_in_generics, f.generic_ty_param());
                        builder_generics_tuple.elems.push_value(f.type_ident());
                    } else if f.ordinal < field.ordinal {
                        // Only add a `build` method that warns about missing `field` if `f` is set. If `f` is not set,
                        // `f`'s `build` method will warn, since it appears earlier in the argument list.
                        builder_generics_tuple
                            .elems
                            .push_value(f.tuplized_type_ty_param());
                    } else if f.ordinal == field.ordinal {
                        builder_generics_tuple.elems.push_value(empty_type());
                    } else {
                        // `f` appears later in the argument list after `field`, so if they are both missing we will
                        // show a warning for `field` and not for `f` - which means this warning should appear whether
                        // or not `f` is set.
                        g.params
                            .insert(index_after_lifetime_in_generics, f.generic_ty_param());
                        builder_generics_tuple.elems.push_value(f.type_ident());
                    }

                    builder_generics_tuple.elems.push_punct(Default::default());
                }
            });

            let index_after_lifetime_in_generics = builder_generics
                .iter()
                .filter(|arg| matches!(arg, syn::GenericArgument::Lifetime(_)))
                .count();
            builder_generics.insert(
                index_after_lifetime_in_generics,
                syn::GenericArgument::Type(builder_generics_tuple.into()),
            );
            let (impl_generics, _, where_clause) = generics.split_for_impl();
            let (_, ty_generics, _) = self.generics.split_for_impl();

            let early_build_error_type_name = syn::Ident::new(
                &format!(
                    "{}_Error_Missing_required_field_{}",
                    builder_name,
                    strip_raw_ident_prefix(field_name.to_string())
                ),
                builder_name.span(),
            );
            let early_build_error_message = format!("Missing required field {field_name}");

            Ok(quote! {
                #[doc(hidden)]
                #[allow(dead_code, non_camel_case_types, non_snake_case)]
                pub enum #early_build_error_type_name {}
                #[doc(hidden)]
                #[allow(dead_code, non_camel_case_types, missing_docs, clippy::panic)]
                impl #impl_generics #builder_name < #( #builder_generics ),* > #where_clause {
                    #[deprecated(
                        note = #early_build_error_message
                    )]
                    pub fn build(self, _: #early_build_error_type_name) -> #name #ty_generics {
                        panic!();
                    }
                }
            })
        }

        pub fn build_method_impl(&self) -> TokenStream {
            let StructInfo {
                ref name,
                ref builder_name,
                ..
            } = *self;

            let generics = self.modify_generics(|g| {
                let index_after_lifetime_in_generics = g
                    .params
                    .iter()
                    .filter(|arg| matches!(arg, syn::GenericParam::Lifetime(_)))
                    .count();
                for field in self.included_fields() {
                    if field.builder_attr.default.is_some() {
                        let trait_ref = syn::TraitBound {
                            paren_token: None,
                            lifetimes: None,
                            modifier: syn::TraitBoundModifier::None,
                            path: syn::PathSegment {
                                ident: self.conversion_helper_trait_name.clone(),
                                arguments: syn::PathArguments::AngleBracketed(
                                    syn::AngleBracketedGenericArguments {
                                        colon2_token: None,
                                        lt_token: Default::default(),
                                        args: make_punctuated_single(syn::GenericArgument::Type(
                                            field.ty.clone(),
                                        )),
                                        gt_token: Default::default(),
                                    },
                                ),
                            }
                            .into(),
                        };
                        let mut generic_param: syn::TypeParam = field.generic_ident.clone().into();
                        generic_param.bounds.push(trait_ref.into());
                        g.params
                            .insert(index_after_lifetime_in_generics, generic_param.into());
                    }
                }
            });
            let (impl_generics, _, _) = generics.split_for_impl();

            let (_, ty_generics, where_clause) = self.generics.split_for_impl();

            let modified_ty_generics = modify_types_generics_hack(&ty_generics, |args| {
                args.insert(
                    0,
                    syn::GenericArgument::Type(
                        type_tuple(self.included_fields().map(|field| {
                            if field.builder_attr.default.is_some() {
                                field.type_ident()
                            } else {
                                field.tuplized_type_ty_param()
                            }
                        }))
                        .into(),
                    ),
                );
            });

            let descructuring = self.included_fields().map(|f| f.name);

            let helper_trait_name = &self.conversion_helper_trait_name;
            // The default of a field can refer to earlier-defined fields, which we handle by
            // writing out a bunch of `let` statements first, which can each refer to earlier ones.
            // This means that field ordering may actually be significant, which isn’t ideal. We could
            // relax that restriction by calculating a DAG of field default dependencies and
            // reordering based on that, but for now this much simpler thing is a reasonable approach.
            let assignments = self.fields.iter().map(|field| {
                let name = &field.name;
                if !field.builder_attr.extends.is_empty() {
                    quote!(let #name = self.#name;)
                } else if let Some(ref default) = field.builder_attr.default {
                    if field.builder_attr.skip {
                        quote!(let #name = #default;)
                    } else {
                        quote!(let #name = #helper_trait_name::into_value(#name, || #default);)
                    }
                } else {
                    quote!(let #name = #name.0;)
                }
            });
            let field_names = self.fields.iter().map(|field| field.name);
            let doc = if self.builder_attr.doc {
                match self.builder_attr.build_method_doc {
                    Some(ref doc) => quote!(#[doc = #doc]),
                    None => {
                        // I’d prefer “a” or “an” to “its”, but determining which is grammatically
                        // correct is roughly impossible.
                        let doc =
                            format!("Finalise the builder and create its [`{name}`] instance");
                        quote!(#[doc = #doc])
                    }
                }
            } else {
                quote!()
            };
            quote!(
                #[allow(dead_code, non_camel_case_types, missing_docs)]
                impl #impl_generics #builder_name #modified_ty_generics #where_clause {
                    #doc
                    pub fn build(self) -> #name #ty_generics {
                        let ( #(#descructuring,)* ) = self.fields;
                        #( #assignments )*
                        #name {
                            #( #field_names ),*
                        }
                    }
                }
            )
        }
    }

    #[derive(Debug, Default)]
    pub struct TypeBuilderAttr {
        /// Whether to show docs for the `TypeBuilder` type (rather than hiding them).
        pub doc: bool,

        /// Docs on the `Type::builder()` method.
        pub builder_method_doc: Option<syn::Expr>,

        /// Docs on the `TypeBuilder` type. Specifying this implies `doc`, but you can just specify
        /// `doc` instead and a default value will be filled in here.
        pub builder_type_doc: Option<syn::Expr>,

        /// Docs on the `TypeBuilder.build()` method. Specifying this implies `doc`, but you can just
        /// specify `doc` instead and a default value will be filled in here.
        pub build_method_doc: Option<syn::Expr>,

        pub field_defaults: FieldBuilderAttr,
    }

    impl TypeBuilderAttr {
        pub fn new(attrs: &[syn::Attribute]) -> Result<TypeBuilderAttr, Error> {
            let mut result = TypeBuilderAttr::default();
            for attr in attrs {
                if path_to_single_string(attr.path()).as_deref() != Some("builder") {
                    continue;
                }

                match &attr.meta {
                    syn::Meta::List(list) => {
                        if list.tokens.is_empty() {
                            continue;
                        }
                    }
                    _ => {
                        continue;
                    }
                }

                let as_expr = attr.parse_args_with(
                    Punctuated::<Expr, syn::Token![,]>::parse_separated_nonempty,
                )?;

                for expr in as_expr.into_iter() {
                    result.apply_meta(expr)?;
                }
            }

            Ok(result)
        }

        fn apply_meta(&mut self, expr: syn::Expr) -> Result<(), Error> {
            match expr {
                syn::Expr::Assign(assign) => {
                    let name = expr_to_single_string(&assign.left)
                        .ok_or_else(|| Error::new_spanned(&assign.left, "Expected identifier"))?;
                    match name.as_str() {
                        "builder_method_doc" => {
                            self.builder_method_doc = Some(*assign.right);
                            Ok(())
                        }
                        "builder_type_doc" => {
                            self.builder_type_doc = Some(*assign.right);
                            self.doc = true;
                            Ok(())
                        }
                        "build_method_doc" => {
                            self.build_method_doc = Some(*assign.right);
                            self.doc = true;
                            Ok(())
                        }
                        _ => Err(Error::new_spanned(
                            &assign,
                            format!("Unknown parameter {name:?}"),
                        )),
                    }
                }
                syn::Expr::Path(path) => {
                    let name = path_to_single_string(&path.path)
                        .ok_or_else(|| Error::new_spanned(&path, "Expected identifier"))?;
                    match name.as_str() {
                        "doc" => {
                            self.doc = true;
                            Ok(())
                        }
                        _ => Err(Error::new_spanned(
                            &path,
                            format!("Unknown parameter {name:?}"),
                        )),
                    }
                }
                syn::Expr::Call(call) => {
                    let subsetting_name = if let syn::Expr::Path(path) = &*call.func {
                        path_to_single_string(&path.path)
                    } else {
                        None
                    }
                    .ok_or_else(|| {
                        let call_func = &call.func;
                        let call_func = quote!(#call_func);
                        Error::new_spanned(
                            &call.func,
                            format!("Illegal builder setting group {call_func}"),
                        )
                    })?;
                    match subsetting_name.as_str() {
                        "field_defaults" => {
                            for arg in call.args {
                                self.field_defaults.apply_meta(arg)?;
                            }
                            Ok(())
                        }
                        _ => Err(Error::new_spanned(
                            &call.func,
                            format!("Illegal builder setting group name {subsetting_name}"),
                        )),
                    }
                }
                _ => Err(Error::new_spanned(expr, "Expected (<...>=<...>)")),
            }
        }
    }
}
