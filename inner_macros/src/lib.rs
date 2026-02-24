extern crate proc_macro2;

use proc_macro2::{Ident, Span};
use proc_macro2_diagnostics::{Diagnostic, Level};
use quote::quote;
use syn::spanned::Spanned;
use syn::{DeriveInput, Expr, Type, parse_quote, parse2, GenericArgument, PathArguments};

fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(path) = ty {
        // Check if the type is an Option
        if let Some(segment) = path.path.segments.first() {
            return segment.ident.to_string() == "Option";
        }
    }
    false
}

fn make_option(ty: &Type) -> Type {
    if is_option_type(ty) {
        ty.clone()
    } else {
        parse_quote! { Option<#ty> }
    }
}

fn is_numeric_type(ty: &Type) -> bool {
    if let Type::Path(path) = ty {
        // Check if the type is an Option
        for segment in &path.path.segments {
            return segment.ident.to_string() == "Numeric" || {
                let mut rv = false;
                if let PathArguments::AngleBracketed(generic) = &segment.arguments {
                    for arg in &generic.args {
                        if let GenericArgument::Type(t) = arg {
                            rv = rv || is_numeric_type(t);
                        }
                    }
                }
                rv
            };
        }
    }
    false
}

/// ### Details
///
/// From a basic database-faithful model, this macro generates structs for create and update DTOs as
/// well as structs for request and response DTOs.
///
/// This macro generates 5 DTOs, named after the original entity. For example, for an entity named
/// `Entry`, this macro generates:
///
/// - NewEntry              - Used to create an entity in the database
/// - CreateEntryRequest    - The user-facing request DTO to create an entity
/// - UpdateEntry           - Used to update an entity in the database
/// - UpdateEntryRequest    - The user-facing request DTO to update an entity
/// - EntryResponse         - The user-facing response DTO from GET APIs
///
/// All you have to do is specify attributes on fields. You can specify 6 different attributes:
/// - NotInResponse
///   - The field is not present in the response DTO
/// - HasDefault
///   - The field is optional in the new DTO and the create request DTO
/// - NotInDatabaseUpdate
/// - NotInCreateRequest
///   - Not in the create request DTO.
///   - Having NotInCreateRequest does not mean we have to have a default. It can be a calculated
///     value.
/// - NotInUpdateRequest
///   - Not in the update request DTO.
/// - HasCalculatedDefault
///   - Optional in create request DTO, but required in new. The back-end should calculate this
///     value and provide it in new.
///   - It does not need to also be optional in update request DTO, because everything in update
///     request DTO is optional anyway.
/// - Id
///   - The field is not present in the database create DTO as it is database-side generated.
/// - RepresentableAsString
///   - This field is present as a string in request and response DTOs.
///   - A side effect is that if the field name ends in `_id`, it is removed.
///   - This allows user-facing requests and responses to rely on names rather than IDs for
///     representing those fields.
///
/// ### Usage
///
/// To specify a field as not present in any request or response, you can use:
/// ```rust
/// #[derive(inner_macros::Entity)]
/// pub struct SomeEntity {
///     #[entity(NotInResponse, Id)]
///     pub some_field: i32,
/// }
/// ```
///
/// Here's an example of the usage of this macro from the project it was initially built to support:
/// ```txt
/// #[derive(Entity, Debug, Queryable, Selectable, Identifiable, Associations,
/// Insertable, Serialize)]
/// #[diesel(table_name = entries)]
/// #[diesel(belongs_to(User))]
/// #[diesel(belongs_to(Source))]
/// #[diesel(belongs_to(Category))]
/// #[diesel(check_for_backend(diesel::pg::Pg))]
/// pub struct Entry {
///     #[entity(NotInResponse, NotInDatabaseUpdate, NotInCreateRequest, Id)]
///     pub id: i32,
///     #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
///     pub user_id: i32,
///     pub description: String,
///     #[entity(RepresentableAsString)]
///     pub category_id: i32,
///     pub amount: Numeric,
///     #[entity(RepresentableAsString)]
///     pub date: NaiveDateTime,
///     #[entity(NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest, HasDefault)]
///     pub created_at: NaiveDateTime,
///     #[entity(RepresentableAsString)]
///     pub currency_id: i32,
///     pub entry_type: EntryType,
///     #[entity(RepresentableAsString)]
///     pub source_id: i32,
///     #[entity(RepresentableAsString)]
///     pub secondary_source_id: Option<i32>,
///     pub conversion_rate: Option<Numeric>,
///     pub conversion_rate_to_fixed: Numeric,
///     #[entity(HasDefault)]
///     pub archived: bool,
/// }
/// ```
///
/// ### Limitations:
/// For now, this macro only works with postgres diesel connections.
/// This macro requires the entity to also be annotated with:
/// ```txt
/// #[diesel(table_name = ... )]
/// ```
#[proc_macro_derive(Entity, attributes(entity))]
pub fn entity_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match entity_macro_internal(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(diag) => diag.emit_as_expr_tokens().into(),
    }
}

fn entity_macro_internal(
    input: proc_macro2::TokenStream,
) -> Result<proc_macro2::TokenStream, Diagnostic> {
    let ast = parse2::<DeriveInput>(input)?;

    let struct_name = &ast.ident;

    #[cfg(feature = "backend")]
    let table_name_vec = &ast
        .attrs
        .iter()
        .filter_map(|el| {
            if el.path().is_ident("diesel") {
                match el.parse_args::<Expr>() {
                    Ok(expr) => match expr {
                        Expr::Assign(assign) => {
                            if let syn::Expr::Path(p) = assign.left.as_ref()
                                && let Some(ident) = p.path.get_ident()
                                && ident == "table_name"
                            {
                                Some(assign.right)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    },
                    Err(_) => None,
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    #[cfg(feature = "backend")]
    let table_name = match table_name_vec.first() {
        Some(t) => t,
        None => {
            return Err(Diagnostic::new(
                Level::Error,
                "No #[diesel(table_name = ...)] attribute encountered",
            ));
        }
    };
    #[cfg(feature = "backend")]
    let new_struct_name = Ident::new(format!("New{struct_name}").as_str(), Span::call_site());
    #[cfg(feature = "backend")]
    let update_struct_name = Ident::new(format!("Update{struct_name}").as_str(), Span::call_site());

    let deny_unknown_vec = &ast
        .attrs
        .iter()
        .filter_map(|el| {
            if el.path().is_ident("serde") {
                match el.parse_args::<Expr>() {
                    Ok(expr) => match expr {
                        Expr::Path(p) => {
                            if let Some(ident) = p.path.get_ident()
                                && ident == "deny_unknown_fields"
                            {
                                Some(Expr::Path(p))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    },
                    Err(_) => None,
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let deny_unknown = if deny_unknown_vec.first().is_some() {
        quote! {
            #[serde(deny_unknown_fields)]
        }
    } else {
        quote! {}
    };

    let create_request_struct_name =
        Ident::new(format!("Create{struct_name}Request").as_str(), Span::call_site());

    let update_request_struct_name =
        Ident::new(format!("Update{struct_name}Request").as_str(), Span::call_site());
    let response_struct_name =
        Ident::new(format!("{struct_name}Response").as_str(), Span::call_site());
    let mut entity_fields = Vec::new();
    let mut new_fields = Vec::new();
    let mut create_request_fields = Vec::new();
    let mut update_fields = Vec::new();
    let mut update_request_fields = Vec::new();
    let mut response_fields = Vec::new();

    if let syn::Data::Struct(data_struct) = ast.data {
        for field in data_struct.fields {
            if let Some(ident) = field.ident {
                let mut push_to_new = true;
                let mut option_in_new = false;
                let mut option_in_create_request = false;
                let mut push_to_create_request = true;
                let mut push_to_update = true;
                let mut push_to_update_request = true;
                let mut push_to_response = true; // Also controls whether it's serialized
                let mut representable_as_name = false;

                for attr in field.attrs {
                    if attr.path().is_ident("entity") {
                        match attr.parse_nested_meta(|meta| {
                            match meta
                                .path
                                .get_ident()
                                .expect("X001: All metas inside entity should be single path")
                                .to_string()
                                .as_str()
                            {
                                "NotInResponse" => {
                                    push_to_response = false;
                                }
                                "HasDefault" => {
                                    // option_in_new forces option_in_create_request, but not
                                    // vice versa.
                                    option_in_new = true;
                                }
                                "HasCalculatedDefault" => {
                                    option_in_create_request = true;
                                }
                                "NotInCreateRequest" => {
                                    push_to_create_request = false;
                                }
                                "NotInUpdateRequest" => {
                                    push_to_update_request = false;
                                }
                                "NotInDatabaseUpdate" => {
                                    push_to_update = false;
                                }
                                "Id" => {
                                    push_to_new = false;
                                }
                                "RepresentableAsString" => {
                                    representable_as_name = true;
                                }
                                other => {
                                    return Err(syn::Error::new(
                                        attr.span(),
                                        format!(
                                            "Unknown meta {other}. Expected a value in the values \
                                             listed in the docblock of this fn"
                                        ),
                                    ));
                                }
                            };

                            Ok(())
                        }) {
                            Ok(_) => {}
                            Err(e) => {
                                return Err(Diagnostic::new(Level::Error, e.to_string()));
                            }
                        };
                    }
                }

                let field_type = field.ty;
                let field_type_opt = make_option(&field_type);

                let name_ident = if representable_as_name {
                    let name_id = ident.to_string();
                    let name = match name_id.strip_suffix("_id") {
                        Some(n) => n,
                        None => name_id.as_str(),
                    };
                    Ident::new(name, Span::call_site())
                } else {
                    ident.clone()
                };

                let name_type: Type = if representable_as_name {
                    if is_option_type(&field_type) {
                        parse_quote! { Option<String> }
                    } else {
                        parse_quote! { String }
                    }
                } else {
                    field_type.clone()
                };

                let response_type = if is_numeric_type(&name_type) {
                    if is_option_type(&name_type) {
                        parse_quote! { Option<Decimal> }
                    } else {
                        parse_quote! { Decimal }
                    }
                } else {
                    name_type.clone()
                };

                if push_to_response {
                    response_fields.push(quote! { pub #name_ident: #response_type });
                }

                let name_type_opt = make_option(&name_type);
                let name_type = if option_in_new { make_option(&name_type) } else { name_type };

                let new_type =
                    if option_in_new { field_type_opt.clone() } else { field_type.clone() };

                let create_request_type = if option_in_create_request {
                    if is_numeric_type(&name_type) {
                        parse_quote! { Option<Decimal> }
                    } else {
                        name_type_opt.clone()
                    }
                } else {
                    if is_numeric_type(&name_type) {
                        if is_option_type(&name_type) {
                            parse_quote! { Option<Decimal> }
                        } else {
                            parse_quote! { Decimal }
                        }
                    } else {
                        name_type.clone()
                    }
                };

                let update_request_type = if is_numeric_type(&name_type) {
                    parse_quote! { Option<Decimal> }
                } else {
                    name_type_opt.clone()
                };

                entity_fields.push(quote! { pub #ident: #field_type });

                if push_to_new {
                    new_fields.push(quote! { pub #ident: #new_type });
                }

                if push_to_create_request {
                    create_request_fields.push(quote! { pub #name_ident: #create_request_type });
                }

                if push_to_update {
                    update_fields.push(quote! { pub #ident: #field_type_opt });
                }

                if push_to_update_request {
                    update_request_fields.push(quote! { pub #name_ident: #update_request_type });
                }
            } else {
                return Err(Diagnostic::new(
                    Level::Error,
                    format!("Non-struct field encountered {:?}", field).as_str(),
                ));
            }
        }
    }

    #[cfg(feature = "backend")]
    let backend = quote! {
        #[cfg(feature = "backend")]
        #[derive(diesel::Insertable)]
        #[diesel(table_name = #table_name)]
        #[diesel(check_for_backend(diesel::pg::Pg))]
        pub struct #new_struct_name {
            #(#new_fields,)*
        }

        #[cfg(feature = "backend")]
        #[derive(diesel::AsChangeset)]
        #[diesel(table_name = #table_name)]
        #[diesel(check_for_backend(diesel::pg::Pg))]
        pub struct #update_struct_name {
            #(#update_fields,)*
        }
    };

    #[cfg(not(feature = "backend"))]
    let backend =  quote! {};

    let expanded = quote! {
        #backend

        #[derive(Debug, Serialize, Deserialize)]
        #deny_unknown
        pub struct #create_request_struct_name {
            #(#create_request_fields,)*
        }

        #[derive(Debug, Serialize, Deserialize)]
        #deny_unknown
        pub struct #update_request_struct_name {
            #(#update_request_fields,)*
        }

        #[derive(Debug, Serialize, Deserialize, Clone)]
        pub struct #response_struct_name {
            #(#response_fields,)*
        }
    };
    // return Err(Diagnostic::new(Level::Error, format!("expanded {}",
    // expanded).as_str()));

    Ok(expanded)
}
