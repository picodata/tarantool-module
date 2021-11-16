use std::convert::TryFrom;

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, parse_quote, Data, DataStruct, DeriveInput, Fields, GenericParam, Generics,
    Index,
};
use hlua::{lua_get, lua_push, dereference_and_corrupt_mut_ref, start_read_table};
use ffi as l_ffi;

#[macro_export]
#[proc_macro_derive(MakeHlua)]
pub fn derive_hlua_features(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let struct_name = &ast.ident;

    let fields = if let syn::Data::Struct(syn::DataStruct {
        fields: syn::Fields::Named(ref fields),
        ..
    }) = ast.data
    {
        fields
    } else {
        panic!("Only support Struct")
    };

    let mut keys = Vec::new();
    let mut idents = Vec::new();
    let mut types = Vec::new();
    let mut numbers : Vec<usize> = Vec::new();
    let mut counter = 0;
    let field_count = fields.named.iter().count();

    for field in fields.named.iter() {
        let field_name: &syn::Ident = field.ident.as_ref().unwrap();
        //let name = &field.key;
        //let literal_key_str = syn::LitStr::new(&name, field.span());
        let type_name = &field.ty;
        keys.push( field_name );
        idents.push(&field.ident);
        types.push(type_name.into_token_stream());
        //numbers.push(counter);
        counter = counter + 1;
    }

    //<Ret as VerifyLuaTuple>::check(
    //unsafe {lua_ffi::lua_settop( raw_lua, stack_restoring_value ); };
    //let stack_before_args = unsafe { lua_ffi::lua_gettop( raw_lua ) as i32 };
    //Result<PushGuard<L>, (Void, L)>
    let expanded = quote! {
        impl<'lua, L> hlua::Push<L> for #struct_name
        where L: hlua::AsMutLua<'lua> {
            type Err = LuaError;      // TODO: use ! instead
            #[inline(always)]
    fn push_to_lua(self, mut mlua: L) -> Result< PushGuard<L>, (LuaError,L)  > {
                let raw_lua = mlua.as_lua().state_ptr();
                let stack_before = unsafe { l_ffi::lua_gettop( raw_lua ) as i32 };
                #(
                    //let ret = <#types as Push>::push_to_lua( #keys , &mut lua );
                    if ! lua_push!(
                             & mut mlua,
                             self.#keys,
                             unsafe {l_ffi::lua_settop( raw_lua, stack_before ); }
                         ) {
                        let erret : (_, L) = ( LuaError::ExecutionError("Push error!!!".to_string()), mlua );
                        return std::result::Result::Err( erret );
                    }
                )*
        unsafe {l_ffi::lua_settop( raw_lua, stack_before ); }
                std::result::Result::Ok( unsafe { hlua::PushGuard::new( mlua, 0 ) } )
            }
        }
        impl<'lua, L> hlua::LuaRead<L> for #struct_name
        where L: AsMutLua<'lua>, #struct_name : Default {
            #[inline(always)]
            fn lua_read_at_position(mut mlua: L, index: i32) -> Result<Self, L> {
                let raw_lua = mlua.as_lua().state_ptr();
        let stack_before = unsafe { l_ffi::lua_gettop( raw_lua ) as i32 };
                let mut ret = Self::default() ;
                if start_read_table( & mut mlua, &index ) {
                    #(
                let local_ret = lua_get!(
                            &mut ьlua,
                            #numbers - #counter,
                            {}, // reaction to success
                            {   // reaction to fail
                            unsafe {l_ffi::lua_settop( raw_lua, stack_before ); }
                            },
                            #types
                        );
                        let error = match local_ret {
                            std::option::Option::None => {
                                true
                            },
                            std::option::Option::Some( ref var ) => {
                                ret.#keys = *var;
                                false
                            }
                        };
                        if error {
                            return std::result::Result::Err(mlua);
                        }
                    )*
                    std::result::Result::Ok( ret )
                } else {
                    std::result::Result::Err(mlua)
                }
            }
        }
    };
    //panic!(expanded.to_string());
    expanded.into()
}