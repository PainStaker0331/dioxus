use proc_macro::TokenStream;
use quote::ToTokens;
use syn::parse_macro_input;

mod fc;
mod htm;
mod ifmt;
mod rsxt;

/// The html! macro makes it easy for developers to write jsx-style markup in their components.
/// We aim to keep functional parity with html templates.
#[proc_macro]
pub fn html(s: TokenStream) -> TokenStream {
    match syn::parse::<htm::HtmlRender>(s) {
        Err(e) => e.to_compile_error().into(),
        Ok(s) => s.to_token_stream().into(),
    }
}

/// The html! macro makes it easy for developers to write jsx-style markup in their components.
/// We aim to keep functional parity with html templates.
#[proc_macro]
pub fn rsx(s: TokenStream) -> TokenStream {
    match syn::parse::<rsxt::RsxRender>(s) {
        Err(e) => e.to_compile_error().into(),
        Ok(s) => s.to_token_stream().into(),
    }
}

/// Label a function or static closure as a functional component.
/// This macro reduces the need to create a separate properties struct.
#[proc_macro_attribute]
pub fn fc(attr: TokenStream, item: TokenStream) -> TokenStream {
    use fc::{function_component_impl, FunctionComponent};

    let item = parse_macro_input!(item as FunctionComponent);

    function_component_impl(item)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

#[proc_macro]
pub fn format_args_f(input: TokenStream) -> TokenStream {
    use ifmt::*;

    let item = parse_macro_input!(input as IfmtInput);

    // #[allow(unused)]
    // const FUNCTION_NAME: &str = "format_args_f";

    // debug_input!(&input);

    ifmt::format_args_f_impl(item)
    // .unwrap_or_else(|err| err.to_compile_error())
    // .into()
}
