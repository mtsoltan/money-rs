#![feature(let_chains)]

extern crate proc_macro2;

use proc_macro2::{Ident, Span};
use proc_macro2_diagnostics::{Diagnostic, Level};
use quote::quote;
use syn::{parse2, parse_quote, spanned::Spanned, DeriveInput, Expr, Type};

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

/// ### Details
///
/// From a basic database-faithful model, this macro generates structs for create and update DTOs
/// as well as structs for request and response DTOs.
///
/// This macro generates 5 DTOs, named after the original entity. For example, for an entity named `Entry`,
/// this macro generates:
/// - NewEntry              - Used to create an entity in the database
/// - CreateEntryRequest    - The user-facing request DTO to create an entity
/// - UpdateEntry           - Used to update an entity in the database
/// - UpdateEntryRequest    - The user-facing request DTO to update an entity
/// - EntryResponse         - The user-facing response DTO from GET APIs
///
/// All you have to do is specify attributes on fields. You can specify 6 different attributes:
/// - NotUpdatable
///   - The field is not present in database update DTO or update request DTO
/// - NotViewable
///   - The field is not present in the response DTO
/// - HasDefault
///   - The field is optional in the new DTO and the create request DTO
/// - NotSettable
///   - The field is present in neither request DTO, and not present in the database update DTO.
///   - It is, however, present in the database create DTO as it should be set by the server.
/// - Id
///   - The field is not present in the database create DTO as it is database-side generated.
/// - RepresentableAsString
///   - This field is present as a string in request and response DTOs.
///   - A side effect is that if the field name ends in `_id`, it is removed.
///   - This allows user-facing requests and responses to rely on names rather than IDs for representing those fields.
///
/// ### Usage
///
/// To specify a field as not present in any request or response, you can use:
/// ```
/// #[entity(NotUpdatable, NotViewable, NotSettable, Id)]
/// ```
///
/// Here's an example of the usage of this macro from the project it was initially built to support:
/// ```rust
/// #[derive(Entity)]
/// #[derive(Debug, Queryable, Selectable, Identifiable, Associations, Insertable, Serialize)]
/// #[diesel(table_name = entries)]
/// #[diesel(belongs_to(User))]
/// #[diesel(belongs_to(Source))]
/// #[diesel(belongs_to(Category))]
/// #[diesel(check_for_backend(diesel::pg::Pg))]
/// pub struct Entry {
///     #[entity(NotUpdatable, NotViewable, NotSettable, Id)]
///     pub id: i32,
///     #[entity(NotUpdatable, NotViewable, NotSettable)]
///     pub user_id: i32,
///     pub description: String,
///     #[entity(RepresentableAsString)]
///     pub category_id: i32,
///     pub amount: f64,
///     #[entity(RepresentableAsString)]
///     pub date: NaiveDateTime,
///     #[entity(NotUpdatable, NotSettable, HasDefault)]
///     pub created_at: NaiveDateTime,
///     #[entity(RepresentableAsString)]
///     pub currency_id: i32,
///     pub entry_type: EntryType,
///     #[entity(RepresentableAsString)]
///     pub source_id: i32,
///     #[entity(RepresentableAsString)]
///     pub secondary_source_id: Option<i32>,
///     pub conversion_rate: Option<f64>,
///     pub conversion_rate_to_fixed: f64,
///     #[entity(HasDefault)]
///     archived: bool,
/// }
/// ```
///
/// ### Limitations:
/// For now, this macro only works with postgres diesel connections.
/// This macro requires the entity to also be annotated with:
/// ```
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
    let table_name = match table_name_vec.first() {
        Some(t) => t,
        None => {
            return Err(Diagnostic::new(
                Level::Error,
                "No #[diesel(table_name = ...)] attribute encountered",
            ));
        }
    };
    let new_struct_name = Ident::new(format!("New{struct_name}").as_str(), Span::call_site());
    let create_request_struct_name = Ident::new(
        format!("Create{struct_name}Request").as_str(),
        Span::call_site(),
    );
    let update_struct_name = Ident::new(format!("Update{struct_name}").as_str(), Span::call_site());
    let update_request_struct_name = Ident::new(
        format!("Update{struct_name}Request").as_str(),
        Span::call_site(),
    );
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
                let mut push_to_create_request = true;
                let mut push_to_update = true;
                let mut push_to_update_request = true;
                let mut push_to_response = true; // Also controls whether it's serialized
                let mut representable_as_name = false;

                for attr in field.attrs {
                    if attr.path().is_ident("entity") {
                        match attr.parse_nested_meta(|meta| {
                            match meta.path.get_ident().expect("All metas inside entity should be single path").to_string().as_str() {
                                "NotUpdatable" => {
                                    push_to_update = false;
                                    push_to_update_request = false;
                                },
                                "NotViewable" => {
                                    push_to_response = false;
                                },
                                "HasDefault" => {
                                    option_in_new = true;
                                },
                                "NotSettable" => {
                                    push_to_create_request = false;
                                    push_to_update = false;
                                    push_to_update_request = false;
                                },
                                "Id" => {
                                    push_to_new = false;
                                },
                                "RepresentableAsString" => {
                                    representable_as_name = true;
                                }
                                other => {
                                    return Err(syn::Error::new(
                                        attr.span(),
                                        format!("Unknown meta {other}. Expected a value in (NotUpdatable, NotViewable, NotSettable, Id, RepresentableAsString, HasDefault)")
                                    ));
                                }
                            };

                            Ok(())
                        }) {
                            Ok(_) => {},
                            Err(e) => { return Err(Diagnostic::new(Level::Error, e.to_string()));}
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

                if push_to_response {
                    response_fields.push(quote! { pub #name_ident: #name_type });
                }

                let name_type = if option_in_new {
                    make_option(&name_type)
                } else {
                    name_type
                };

                let new_type = if option_in_new {
                    field_type_opt.clone()
                } else {
                    field_type.clone()
                };

                let name_type_opt = make_option(&name_type);

                entity_fields.push(quote! { pub #ident: #field_type });

                if push_to_new {
                    new_fields.push(quote! { pub #ident: #new_type });
                }

                if push_to_create_request {
                    create_request_fields.push(quote! { pub #name_ident: #name_type });
                }

                if push_to_update {
                    update_fields.push(quote! { pub #ident: #field_type_opt });
                }

                if push_to_update_request {
                    update_request_fields.push(quote! { pub #name_ident: #name_type_opt });
                }
            } else {
                return Err(Diagnostic::new(
                    Level::Error,
                    format!("Non-struct field encountered {:?}", field).as_str(),
                ));
            }
        }
    }

    let expanded = quote! {
        #[derive(Insertable)]
        #[diesel(table_name = #table_name)]
        #[diesel(check_for_backend(diesel::pg::Pg))]
        pub struct #new_struct_name {
            #(#new_fields,)*
        }

        #[derive(Debug, Serialize, Deserialize)]
        pub struct #create_request_struct_name {
            #(#create_request_fields,)*
        }

        #[derive(AsChangeset)]
        #[diesel(table_name = #table_name)]
        #[diesel(check_for_backend(diesel::pg::Pg))]
        pub struct #update_struct_name {
            #(#update_fields,)*
        }

        #[derive(Debug, Serialize, Deserialize)]
        pub struct #update_request_struct_name {
            #(#update_request_fields,)*
        }

        #[derive(Debug, Serialize, Deserialize)]
        pub struct #response_struct_name {
            #(#response_fields,)*
        }
    };
    // return Err(Diagnostic::new(Level::Error, format!("expanded {}", expanded).as_str()));

    Ok(expanded)
}
