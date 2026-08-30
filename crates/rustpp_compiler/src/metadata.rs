use std::fmt::Write;

use crate::hir::{BaseVisibility, LifecycleStep, MethodKind, Module};

pub const ABI_VERSION: u32 = 1;

pub fn stable_class_id(abi_identity: &str, class_name: &str) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut hash = OFFSET;
    let identity = format!(
        "rustpp-abi-{ABI_VERSION}:{}:{abi_identity}:{}:{class_name}",
        abi_identity.len(),
        class_name.len()
    );
    for byte in identity.bytes() {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                write!(output, "\\u{:04x}", u32::from(character)).unwrap();
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

pub fn emit(abi_identity: &str, module: &Module) -> String {
    let class_name = |id| {
        module
            .classes
            .iter()
            .find(|class| class.id == id)
            .map(|class| class.name.as_str())
            .expect("metadata class reference must resolve")
    };
    let mut output = String::new();
    writeln!(output, "{{").unwrap();
    writeln!(output, "  \"format\": \"rustpp-bootstrap-metadata\",").unwrap();
    writeln!(output, "  \"abi_version\": {ABI_VERSION},").unwrap();
    writeln!(output, "  \"source\": {},", json_string(abi_identity)).unwrap();
    writeln!(output, "  \"abi_identity\": {},", json_string(abi_identity)).unwrap();
    writeln!(output, "  \"value_classes\": [").unwrap();
    for (index, class) in module.value_classes.iter().enumerate() {
        writeln!(output, "    {{").unwrap();
        writeln!(output, "      \"name\": {},", json_string(&class.name)).unwrap();
        writeln!(
            output,
            "      \"generics\": {},",
            json_string(&format!(
                "{} {}",
                class.generics.declaration, class.generics.where_clause
            ))
        )
        .unwrap();
        writeln!(output, "      \"fields\": [").unwrap();
        for (field_index, field) in class.fields.iter().enumerate() {
            writeln!(
                output,
                "        {{\"name\": {}, \"type\": {}, \"public\": {}}}{}",
                json_string(&field.name),
                json_string(&field.ty.source),
                field.public,
                if field_index + 1 == class.fields.len() {
                    ""
                } else {
                    ","
                }
            )
            .unwrap();
        }
        writeln!(output, "      ],").unwrap();
        writeln!(output, "      \"has_drop\": {}", class.drop_body.is_some()).unwrap();
        writeln!(
            output,
            "    }}{}",
            if index + 1 == module.value_classes.len() {
                ""
            } else {
                ","
            }
        )
        .unwrap();
    }
    writeln!(output, "  ],").unwrap();
    writeln!(output, "  \"classes\": [").unwrap();
    for (index, class) in module.classes.iter().enumerate() {
        writeln!(output, "    {{").unwrap();
        writeln!(output, "      \"name\": {},", json_string(&class.name)).unwrap();
        writeln!(
            output,
            "      \"class_id\": {},",
            json_string(&format!(
                "{:032x}",
                stable_class_id(abi_identity, &class.name)
            ))
        )
        .unwrap();
        writeln!(output, "      \"abstract\": {},", class.abstract_).unwrap();
        writeln!(output, "      \"final\": {},", class.final_).unwrap();
        writeln!(output, "      \"bases\": [").unwrap();
        for (base_index, base) in class.bases.iter().enumerate() {
            let visibility = match base.visibility {
                BaseVisibility::Public => "public",
                BaseVisibility::Protected => "protected",
                BaseVisibility::Private => "private",
            };
            writeln!(
                output,
                "        {{\"class\": {}, \"visibility\": {}}}{}",
                json_string(class_name(base.class)),
                json_string(visibility),
                if base_index + 1 == class.bases.len() {
                    ""
                } else {
                    ","
                }
            )
            .unwrap();
        }
        writeln!(output, "      ],").unwrap();
        writeln!(output, "      \"fields\": [").unwrap();
        for (field_index, field) in class.fields.iter().enumerate() {
            writeln!(
                output,
                "        {{\"name\": {}, \"type\": {}, \"public\": {}}}{}",
                json_string(&field.name),
                json_string(&field.ty.source),
                field.public,
                if field_index + 1 == class.fields.len() {
                    ""
                } else {
                    ","
                }
            )
            .unwrap();
        }
        writeln!(output, "      ],").unwrap();
        writeln!(output, "      \"constructor_params\": [").unwrap();
        for (param_index, param) in class.constructor.params.iter().enumerate() {
            writeln!(
                output,
                "        {{\"name\": {}, \"type\": {}}}{}",
                json_string(&param.name),
                json_string(&param.ty.source),
                if param_index + 1 == class.constructor.params.len() {
                    ""
                } else {
                    ","
                }
            )
            .unwrap();
        }
        writeln!(output, "      ],").unwrap();
        writeln!(output, "      \"methods\": [").unwrap();
        for (method_index, method) in class.methods.iter().enumerate() {
            let kind = match method.kind {
                MethodKind::NonVirtual => "nonvirtual",
                MethodKind::Virtual => "virtual",
                MethodKind::Override { final_: false } => "override",
                MethodKind::Override { final_: true } => "final_override",
            };
            let slot = method.slot.map_or_else(
                || "null".to_owned(),
                |slot| json_string(&format!("{}:{}", class_name(slot.owner), slot.index)),
            );
            writeln!(
                output,
                "        {{\"name\": {}, \"signature\": {}, \"public\": {}, \"kind\": {}, \"slot\": {slot}}}{}",
                json_string(&method.name),
                json_string(&method.signature),
                method.public,
                json_string(kind),
                if method_index + 1 == class.methods.len() { "" } else { "," }
            )
            .unwrap();
        }
        writeln!(output, "      ],").unwrap();
        for (name, steps, trailing) in [
            ("activation", &class.lifecycle.activation, true),
            ("deactivation", &class.lifecycle.deactivation, false),
        ] {
            write!(output, "      \"{name}\": [").unwrap();
            for (step_index, step) in steps.iter().enumerate() {
                let id = match step {
                    LifecycleStep::ActivateClass(id) | LifecycleStep::DeactivateClass(id) => *id,
                };
                if step_index > 0 {
                    output.push_str(", ");
                }
                output.push_str(&json_string(class_name(id)));
            }
            writeln!(output, "]{}", if trailing { "," } else { "" }).unwrap();
        }
        writeln!(
            output,
            "    }}{}",
            if index + 1 == module.classes.len() {
                ""
            } else {
                ","
            }
        )
        .unwrap();
    }
    writeln!(output, "  ]").unwrap();
    writeln!(output, "}}").unwrap();
    output
}
