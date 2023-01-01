extern crate proc_macro;

mod sorted_slice;

use proc_macro::TokenStream;
use quote::quote;
use sorted_slice::StrSlice;
use syn::{self, parse_macro_input};

/// Sorts a slice of string literals at compile time.
#[proc_macro]
pub fn sorted_str_slice(input: TokenStream) -> TokenStream {
    let slice: StrSlice = parse_macro_input!(input as StrSlice);
    let strings = slice.map.values();
    quote!([#(#strings, )*]).into()
}

/// Derive's the state from any members that implement the Pass trait
#[proc_macro_derive(State, attributes(skip, skip_clone))]
pub fn state_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_derive_macro(&ast)
}

fn impl_derive_macro(ast: &syn::DeriveInput) -> TokenStream {
    let type_name = &ast.ident;
    let fields: Vec<_> = match &ast.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(e) => &e.named,
            syn::Fields::Unnamed(_) => todo!("unnamed fields"),
            syn::Fields::Unit => todo!("unit structs"),
        }
        .iter()
        .collect(),
        _ => unimplemented!(),
    };

    let clone_or_default = fields.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let skip_clone = field
            .attrs
            .iter()
            .any(|attr| attr.path.is_ident("skip_clone"));
        if skip_clone {
            quote! {
                Default::default()
            }
        } else {
            quote! {
                self.#field_name.clone()
            }
        }
    });

    let non_clone_types = fields
        .iter()
        .filter(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path.is_ident("skip_clone"))
        })
        .map(|field| &field.ty);

    let names = fields.iter().map(|field| field.ident.as_ref().unwrap());

    let types = fields
        .iter()
        .filter(|field| !field.attrs.iter().any(|attr| attr.path.is_ident("skip")))
        .map(|field| &field.ty);

    let gen = quote! {
        impl dioxus_native_core::State for #type_name {
            fn create_passes() -> Box<[dioxus_native_core::TypeErasedPass<Self>]> {
                Box::new([
                    #(
                        <#types as dioxus_native_core::Pass>::to_type_erased()
                    ),*
                ])
            }

            fn clone_or_default(&self) -> Self {
                Self {
                    #(
                        #names: #clone_or_default
                    ),*
                }
            }

            fn non_clone_members() -> Box<[std::any::TypeId]> {
                Box::new([
                    #(
                        std::any::TypeId::of::<#non_clone_types>()
                    ),*
                ])
            }
        }
    };

    gen.into()
}

/// Derive's the state from any elements that have a node_dep_state, child_dep_state, parent_dep_state, or state attribute.
#[proc_macro_derive(AnyMapLike)]
pub fn anymap_like_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_anymap_like_derive_macro(&ast)
}

fn impl_anymap_like_derive_macro(ast: &syn::DeriveInput) -> TokenStream {
    let type_name = &ast.ident;
    let fields: Vec<_> = match &ast.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(e) => &e.named,
            syn::Fields::Unnamed(_) => todo!("unnamed fields"),
            syn::Fields::Unit => todo!("unit structs"),
        }
        .iter()
        .collect(),
        _ => unimplemented!(),
    };

    let names: Vec<_> = fields
        .iter()
        .map(|field| field.ident.as_ref().unwrap())
        .collect();
    let types: Vec<_> = fields.iter().map(|field| &field.ty).collect();

    let gen = quote! {
        impl dioxus_native_core::AnyMapLike for #type_name {
            fn get<T: std::any::Any>(&self) -> Option<&T> {
                #(
                    if std::any::TypeId::of::<T>() == std::any::TypeId::of::<#types>() {
                        return unsafe { Some(&*(&self.#names as *const _ as *const T)) }
                    }
                )*
                None
            }

            fn get_mut<T: std::any::Any>(&mut self) -> Option<&mut T> {
                #(
                    if std::any::TypeId::of::<T>() == std::any::TypeId::of::<#types>() {
                        return unsafe { Some(&mut *(&mut self.#names as *mut _ as *mut T)) }
                    }
                )*
                None
            }
        }
    };

    gen.into()
}
