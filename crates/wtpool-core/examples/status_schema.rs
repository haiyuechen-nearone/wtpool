//! Prints the JSON Schema of the `wtpool status` document, from which the VS Code extension
//! generates its types and validator (S7).

fn main() {
    print!("{}", wtpool_core::status::status_schema_text());
}
