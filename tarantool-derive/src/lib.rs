use std::convert::TryFrom;

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, parse_quote, Data, DataStruct, DeriveInput, Fields, GenericParam, Generics,
    Index,
};

#[proc_macro_derive(ToLuaTable)]
pub fn derive_to_lua_table(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let generics = add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let out = match input.data {
        Data::Struct(data_struct) => {
            let fields_count =
                i32::try_from(data_struct.fields.len()).expect("can't calculate fields count");

            let to_lua_table_code = gen_to_lua_table(&data_struct);
            let push_fields_code = gen_push_fields(&data_struct);

            quote! {
                impl #impl_generics ToLuaTable for #name #ty_generics #where_clause {
                    fn to_lua_table(&self) -> Result<(), ::tarantool::lua::ToLuaConversionError> {
                        #to_lua_table_code
                    }

                    fn push_fields(&self, state: &::tarantool::lua::LuaState) ->
                        Result<(), ::tarantool::lua::ToLuaConversionError>
                    {
                        #push_fields_code
                    }

                    fn fields_count(&self) -> i32 {
                        #fields_count
                    }
                }
            }
        }
        _ => {
            quote! {
                compile_error!("Only structs can be converted to lua table");
            }
        }
    };

    proc_macro::TokenStream::from(out)
}

fn add_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param.bounds.push(parse_quote!(heapsize::HeapSize));
        }
    }
    generics
}

fn gen_to_lua_table(_data_struct: &DataStruct) -> TokenStream {
    quote! {
        unimplemented!()
    }
}

fn gen_push_fields(data_struct: &DataStruct) -> TokenStream {
    match data_struct.fields {
        Fields::Named(ref fields) => {
            let statements = fields.named.iter().map(|field| {
                let name = &field.ident;
                quote_spanned! {
                    field.span() => ::tarantool::lua::ToLuaValue::push_lua_value(&self.#name, state)?;
                }
            });

            quote! {
                #(#statements)*
                Ok(())
            }
        }

        Fields::Unnamed(ref fields) => {
            let statements = fields.unnamed.iter().enumerate().map(|(i, field)| {
                let index = Index::from(i);
                quote_spanned! {
                    field.span() => ::tarantool::lua::ToLuaValue::push_lua_value(&self.#index, state)?;
                }
            });

            quote! {
                #(#statements)*
                Ok(())
            }
        }

        Fields::Unit => {
            quote!(Ok(()))
        }
    }
}
