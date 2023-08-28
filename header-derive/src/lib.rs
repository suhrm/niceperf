extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use syn::parse_macro_input;

#[proc_macro_derive(Logging)]
pub fn logging_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_logging(&ast)
}
fn impl_logging(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let fields = match &ast.data {
        syn::Data::Struct(syn::DataStruct {
            fields: syn::Fields::Named(syn::FieldsNamed { ref named, .. }),
            ..
        }) => named,
        _ => panic!("Only Structs are supported"),
    };
    let mut field_names = Vec::new();
    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        field_names.push(field_name);
    }

    let log_format = field_names
        .iter()
        .map(|_| "{}".to_string())
        .collect::<Vec<_>>()
        .join(",");
    let header_format = field_names
        .iter()
        .map(|field_name| format!("{}", field_name))
        .collect::<Vec<_>>()
        .join(",");

    let expanded = quote! {
        impl Logging for #name {
            fn header(&self) -> String {
                concat!{#header_format,"\n"}.to_string()
            }
        }

        impl std::fmt::Display for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!(#log_format,"\n"), #(self.#field_names),*)

            }
        }
    };
    eprintln!("{}", expanded);
    expanded.into()
}
