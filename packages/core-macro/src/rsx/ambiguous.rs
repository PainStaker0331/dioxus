//! Parse anything that has a pattern of < Ident, Bracket >
//! ========================================================
//!
//! Whenever a `name {}` pattern emerges, we need to parse it into an element, a component, or a fragment.
//! This feature must support:
//! - Namepsaced/pathed components
//! - Differentiating between built-in and custom elements

use super::*;

use proc_macro2::TokenStream as TokenStream2;
use quote::ToTokens;
use syn::{
    parse::{Parse, ParseStream},
    Error, Ident, LitStr, Result, Token,
};

pub enum AmbiguousElement {
    Element(Element),
    Component(Component),
}

impl Parse for AmbiguousElement {
    fn parse(input: ParseStream) -> Result<Self> {
        // Try to parse as an absolute path and immediately defer to the componetn
        if input.peek(Token![::]) {
            return input
                .parse::<Component>()
                .map(|c| AmbiguousElement::Component(c));
        }

        // If not an absolute path, then parse the ident and check if it's a valid tag

        if let Ok(pat) = input.fork().parse::<syn::Path>() {
            if pat.segments.len() > 1 {
                return input
                    .parse::<Component>()
                    .map(|c| AmbiguousElement::Component(c));
            }
        }

        // if input.peek(Ident) {
        //     let name_str = input.fork().parse::<Ident>().unwrap().to_string();
        // } else {
        // }

        if let Ok(name) = input.fork().parse::<Ident>() {
            let name_str = name.to_string();

            match is_valid_tag(&name_str) {
                true => input
                    .parse::<Element>()
                    .map(|c| AmbiguousElement::Element(c)),
                false => {
                    let first_char = name_str.chars().next().unwrap();
                    if first_char.is_ascii_uppercase() {
                        input
                            .parse::<Component>()
                            .map(|c| AmbiguousElement::Component(c))
                    } else {
                        let name = input.parse::<Ident>().unwrap();
                        Err(Error::new(
                            name.span(),
                            "Components must be uppercased, perhaps you mispelled a html tag",
                        ))
                    }
                }
            }
        } else {
            if input.peek(LitStr) {
                panic!("it's actually a litstr");
            }
            Err(Error::new(input.span(), "Not a valid Html tag"))
        }
    }
}

impl ToTokens for AmbiguousElement {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        match self {
            AmbiguousElement::Element(el) => el.to_tokens(tokens),
            AmbiguousElement::Component(comp) => comp.to_tokens(tokens),
        }
    }
}
