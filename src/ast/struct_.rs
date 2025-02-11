use crate::{ast::statement::DataType, source::PositionContainer};

/// Collection of fields.
#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct Struct {
	/// The name of the struct.
	pub name: PositionContainer<String>,
	/// The fields of the struct.
	pub fields: Vec<Field>,
}

/// A struct field consists of a name and a type that specify a field of a struct.
#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct Field {
	/// The name of the struct field.
	pub name: PositionContainer<String>,
	/// The type of the field, e.g. a int, a struct or a pointer.
	pub data_type: PositionContainer<DataType>,
}
