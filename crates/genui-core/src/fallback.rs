//! Deterministic CommonMark fallback derived from IR and `to_text` (SPEC §4.7).
use crate::{
    ir::{Ir, Node, NodeId, Prop, PropId, PropValue},
    r#lib::{ComponentDef, Library},
    value::Value,
};
use std::collections::BTreeSet;

/// Render one message's IR without evaluation. Dynamic props contribute empty text;
/// M4 supplies their evaluated values through the same templates.
pub fn render(ir: &Ir, library: &Library) -> String {
    let Some(root) = ir.root else {
        return String::new();
    };
    let mut seen = BTreeSet::new();
    node_text(ir, library, root, &mut seen)
}

fn node_text(ir: &Ir, library: &Library, id: NodeId, seen: &mut BTreeSet<NodeId>) -> String {
    if !seen.insert(id) {
        return String::new();
    }
    let text = ir
        .nodes
        .get(&id)
        .and_then(|node| {
            library
                .components()
                .get(node.kind.0 as usize)
                .map(|def| (node, def))
        })
        .map_or_else(String::new, |(node, def)| {
            template(ir, library, node, def, seen)
        });
    seen.remove(&id);
    text
}

fn template(
    ir: &Ir,
    library: &Library,
    node: &Node,
    def: &ComponentDef,
    seen: &mut BTreeSet<NodeId>,
) -> String {
    let mut text = def.to_text.clone();
    for (index, arg) in def.args.iter().enumerate() {
        if arg.ty.slot().is_some() {
            continue;
        }
        let value = match node.props.get(&PropId(index as u16)) {
            Some(Prop::Static(PropValue::Value(value))) => display(value),
            Some(Prop::Static(PropValue::Token { value, .. })) => display(value),
            Some(Prop::Static(PropValue::Binding(_))) | Some(Prop::Dynamic(_)) | None => {
                String::new()
            }
            Some(Prop::Static(PropValue::Action(_))) => String::new(),
        };
        text = text.replace(&format!("{{{}}}", arg.name), &value);
    }
    for slot in &node.slots {
        let separator = if def.name == "Row" {
            " \u{00b7} "
        } else {
            "\n"
        };
        let children = slot
            .children
            .iter()
            .map(|child| node_text(ir, library, *child, seen))
            .collect::<Vec<_>>()
            .join(separator);
        text = text.replace(&format!("{{{}}}", slot.name), &children);
    }
    text.replace("{value}", "").replace("{values}", "")
}

fn display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Str(text) => text.text.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::List(_) | Value::Map(_) | Value::Ref(_) => {
            crate::canonical::encode(&value.plain_json())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        r#lib::{ComponentDef, GroupDef, Library, Role, T},
        validate::{Options, validate},
    };

    fn library() -> Library {
        Library::builder("test", "0.1.0")
            .group(GroupDef::new("Test", 1, ""))
            .component(
                ComponentDef::new("Column", Role::Group)
                    .group("Test")
                    .doc("column")
                    .req("children", T::nodes())
                    .to_text("{children}"),
            )
            .component(
                ComponentDef::new("Row", Role::Group)
                    .group("Test")
                    .doc("row")
                    .req("children", T::nodes())
                    .to_text("{children}"),
            )
            .component(
                ComponentDef::new("Text", Role::Paragraph)
                    .group("Test")
                    .doc("text")
                    .req("text", T::text())
                    .opt("tone", T::text(), "neutral")
                    .to_text("{text}"),
            )
            .build()
            .unwrap()
    }

    #[test]
    fn templates_are_used_in_slot_order() {
        let library = library();
        let validation = validate(
            b"root = Column([Text(\"one\"), Row([Text(\"two\"), Text(\"three\")])])\n",
            &library,
            &Options::default(),
        );
        assert_eq!(render(&validation.ir, &library), "one\ntwo \u{00b7} three");
    }

    #[test]
    fn dynamic_props_contribute_empty_text_until_evaluation() {
        let library = library();
        let validation = validate(
            b"$show = true\nroot = Text(if($show, \"a\", \"b\"))\n",
            &library,
            &Options::default(),
        );
        assert_eq!(render(&validation.ir, &library), "");
    }
}
