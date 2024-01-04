use std::fmt::{Display, Formatter};

use super::*;

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens, TokenStreamExt};
use syn::{
    parse::{Parse, ParseBuffer, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Ident, LitStr, Result, Token,
};

// =======================================
// Parse the VNode::Element type
// =======================================
#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct Element {
    pub name: ElementName,
    pub key: Option<IfmtInput>,
    pub attributes: Vec<ElementAttrNamed>,
    pub merged_attributes: Vec<ElementAttrNamed>,
    pub children: Vec<BodyNode>,
    pub brace: syn::token::Brace,
}

impl Parse for Element {
    fn parse(stream: ParseStream) -> Result<Self> {
        let el_name = ElementName::parse(stream)?;

        // parse the guts
        let content: ParseBuffer;
        let brace = syn::braced!(content in stream);

        let mut attributes: Vec<ElementAttrNamed> = vec![];
        let mut children: Vec<BodyNode> = vec![];
        let mut key = None;

        // parse fields with commas
        // break when we don't get this pattern anymore
        // start parsing bodynodes
        // "def": 456,
        // abc: 123,
        loop {
            // Parse the raw literal fields
            if content.peek(LitStr) && content.peek2(Token![:]) && !content.peek3(Token![:]) {
                let name = content.parse::<LitStr>()?;
                let ident = name.clone();

                content.parse::<Token![:]>()?;

                let value = content.parse::<ElementAttrValue>()?;
                attributes.push(ElementAttrNamed {
                    el_name: el_name.clone(),
                    attr: ElementAttr {
                        name: ElementAttrName::Custom(name),
                        value,
                    },
                });

                if content.is_empty() {
                    break;
                }

                if content.parse::<Token![,]>().is_err() {
                    missing_trailing_comma!(ident.span());
                }
                continue;
            }

            if content.peek(Ident) && content.peek2(Token![:]) && !content.peek3(Token![:]) {
                let name = content.parse::<Ident>()?;

                let name_str = name.to_string();
                content.parse::<Token![:]>()?;

                // The span of the content to be parsed,
                // for example the `hi` part of `class: "hi"`.
                let span = content.span();

                if name_str.starts_with("on") {
                    attributes.push(ElementAttrNamed {
                        el_name: el_name.clone(),
                        attr: ElementAttr {
                            name: ElementAttrName::BuiltIn(name),
                            value: ElementAttrValue::EventTokens(content.parse()?),
                        },
                    });
                } else {
                    match name_str.as_str() {
                        "key" => {
                            key = Some(content.parse()?);
                        }
                        _ => {
                            let value = content.parse::<ElementAttrValue>()?;
                            attributes.push(ElementAttrNamed {
                                el_name: el_name.clone(),
                                attr: ElementAttr {
                                    name: ElementAttrName::BuiltIn(name),
                                    value,
                                },
                            });
                        }
                    }
                }

                if content.is_empty() {
                    break;
                }

                // todo: add a message saying you need to include commas between fields
                if content.parse::<Token![,]>().is_err() {
                    missing_trailing_comma!(span);
                }
                continue;
            }

            break;
        }

        // Deduplicate any attributes that can be combined
        // For example, if there are two `class` attributes, combine them into one
        let mut merged_attributes: Vec<ElementAttrNamed> = Vec::new();
        for attr in &attributes {
            if let Some(old_attr_index) = merged_attributes
                .iter()
                .position(|a| a.attr.name == attr.attr.name)
            {
                let old_attr = &mut merged_attributes[old_attr_index];
                if let Some(combined) = old_attr.try_combine(attr) {
                    *old_attr = combined;
                }
            } else {
                merged_attributes.push(attr.clone());
            }
        }

        while !content.is_empty() {
            if (content.peek(LitStr) && content.peek2(Token![:])) && !content.peek3(Token![:]) {
                attr_after_element!(content.span());
            }

            if (content.peek(Ident) && content.peek2(Token![:])) && !content.peek3(Token![:]) {
                attr_after_element!(content.span());
            }

            children.push(content.parse::<BodyNode>()?);
            // consume comma if it exists
            // we don't actually care if there *are* commas after elements/text
            if content.peek(Token![,]) {
                let _ = content.parse::<Token![,]>();
            }
        }

        Ok(Self {
            key,
            name: el_name,
            attributes,
            merged_attributes,
            children,
            brace,
        })
    }
}

impl ToTokens for Element {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let name = &self.name;
        let children = &self.children;

        let key = match &self.key {
            Some(ty) => quote! { Some(#ty) },
            None => quote! { None },
        };

        let listeners = self
            .merged_attributes
            .iter()
            .filter(|f| matches!(f.attr.value, ElementAttrValue::EventTokens { .. }));

        let attr = self
            .merged_attributes
            .iter()
            .filter(|f| !matches!(f.attr.value, ElementAttrValue::EventTokens { .. }));

        tokens.append_all(quote! {
            __cx.element(
                #name,
                __cx.bump().alloc([ #(#listeners),* ]),
                __cx.bump().alloc([ #(#attr),* ]),
                __cx.bump().alloc([ #(#children),* ]),
                #key,
            )
        });
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub enum ElementName {
    Ident(Ident),
    Custom(LitStr),
}

impl ElementName {
    pub(crate) fn tag_name(&self) -> TokenStream2 {
        match self {
            ElementName::Ident(i) => quote! { dioxus_elements::#i::TAG_NAME },
            ElementName::Custom(s) => quote! { #s },
        }
    }
}

impl ElementName {
    pub fn span(&self) -> Span {
        match self {
            ElementName::Ident(i) => i.span(),
            ElementName::Custom(s) => s.span(),
        }
    }
}

impl PartialEq<&str> for ElementName {
    fn eq(&self, other: &&str) -> bool {
        match self {
            ElementName::Ident(i) => i == *other,
            ElementName::Custom(s) => s.value() == *other,
        }
    }
}

impl Display for ElementName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ElementName::Ident(i) => write!(f, "{}", i),
            ElementName::Custom(s) => write!(f, "{}", s.value()),
        }
    }
}

impl Parse for ElementName {
    fn parse(stream: ParseStream) -> Result<Self> {
        let raw = Punctuated::<Ident, Token![-]>::parse_separated_nonempty(stream)?;
        if raw.len() == 1 {
            Ok(ElementName::Ident(raw.into_iter().next().unwrap()))
        } else {
            let span = raw.span();
            let tag = raw
                .into_iter()
                .map(|ident| ident.to_string())
                .collect::<Vec<_>>()
                .join("-");
            let tag = LitStr::new(&tag, span);
            Ok(ElementName::Custom(tag))
        }
    }
}

impl ToTokens for ElementName {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        match self {
            ElementName::Ident(i) => tokens.append_all(quote! { dioxus_elements::#i }),
            ElementName::Custom(s) => tokens.append_all(quote! { #s }),
        }
    }
}
