//! Compiling a view file into a [`CompiledView`].
//!
//! Two passes. The first walks the file once, checking every node against
//! the catalog and binding every value by D23's rules; what can only be
//! judged with the whole file in hand (a `use` naming a template defined
//! below it, a cycle of templates, an `each` nested through a template, a
//! lookup table, a `show` naming a tab) is recorded on the way and judged
//! by the second.

use std::collections::{BTreeMap, BTreeSet};

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use uze_contract::binding::{Path, Scope, TextPart, binding_of, is_field_name, parse_text};
use uze_contract::catalog::{CATALOG, COMMON_PROPS, NodeSpec};
use uze_contract::compiled::Span;
use uze_contract::{
    Binding, CompiledView, Condition, Item, Lookup, Node, NodeName, PropKind, RoleName,
};

use super::document::{
    Entries, Place, Report, accepts, count, entries, nearest, refuse_extra, spelled,
};

/// How deep `each` may nest (D23).
pub const EACH_MAX: usize = 2;
const DIRECTIVES: &[&str] = &["template", "use", "lookup"];
const CONDITIONS: &[&str] = &["when", "unless", "show", "show-any"];

pub fn compile(document: &KdlDocument, file: &str, report: &mut Report<'_>) -> CompiledView {
    let mut compiler = Compiler {
        report,
        view: CompiledView {
            file: file.to_owned(),
            ..CompiledView::default()
        },
        uses: Vec::new(),
        templates: BTreeMap::new(),
        tables: Vec::new(),
        shows: Vec::new(),
        states: BTreeMap::new(),
    };
    for node in document.nodes() {
        match node.name().value() {
            "template" => compiler.template(node),
            "lookup" => compiler.lookup(node),
            _ => {
                let place = Within {
                    each: 0,
                    template: None,
                };
                if let Some(item) = compiler.item(node, &place, None) {
                    compiler.view.items.push(item);
                }
            }
        }
    }
    if compiler.view.items.is_empty() && document.nodes().iter().all(is_declaration) {
        compiler
            .report
            .whole("draws nothing: it declares no catalog node", None);
    }
    compiler.finish();
    compiler.view
}

/// Whether `kind` may stand at the top of a view: what any container
/// holds, which leaves out the parts of one (`Mark`, `Segment`, `Tab`).
fn top_level(kind: NodeName) -> bool {
    NodeName::Stack
        .spec()
        .children
        .is_some_and(|held| held.contains(&kind))
}

fn is_declaration(node: &KdlNode) -> bool {
    matches!(node.name().value(), "template" | "lookup")
}

/// Where a node sits: how many `each` enclose it, and in which template.
#[derive(Clone)]
struct Within {
    each: usize,
    template: Option<String>,
}

struct UseSite {
    template: String,
    within: Within,
    offset: usize,
}

struct TemplateFacts {
    offset: usize,
    /// The deepest `each` its own nodes reach, without its `use`s.
    each: usize,
}

/// A host-held value a node declares with `id`, and the values it can
/// take when they are known (a `Tabs`' tabs).
struct State {
    values: Option<BTreeSet<String>>,
}

struct Compiler<'r, 'a> {
    report: &'r mut Report<'a>,
    view: CompiledView,
    uses: Vec<UseSite>,
    templates: BTreeMap<String, TemplateFacts>,
    /// Every lookup table a binding names, and where.
    tables: Vec<(String, usize)>,
    /// Every `show` pair, and where.
    shows: Vec<(String, String, usize)>,
    states: BTreeMap<String, State>,
}

impl Compiler<'_, '_> {
    /// One entry of a node list: a `use` or a catalog node, when `allowed`
    /// (the parent's children, `None` for any) admits it.
    fn item(
        &mut self,
        node: &KdlNode,
        within: &Within,
        parent: Option<(NodeName, &[NodeName])>,
    ) -> Option<Item> {
        let name = node.name().value();
        match name {
            "use" => self.use_template(node, within),
            "template" | "lookup" => {
                self.report.on(
                    node,
                    format!("`{name}` is declared only at the top of a view"),
                    Some("move it out of this block".to_owned()),
                );
                None
            }
            _ => {
                let Some(kind) = NodeName::parse(name) else {
                    self.unknown_node(node, name);
                    return None;
                };
                if within.template.is_none() && parent.is_none() && !top_level(kind) {
                    let parents: Vec<&str> = CATALOG
                        .iter()
                        .filter(|spec| spec.children.is_some_and(|held| held.contains(&kind)))
                        .map(|spec| spec.name.as_str())
                        .collect();
                    self.report.on(
                        node,
                        format!("`{kind}` is drawn only inside {}", parents.join(" or ")),
                        Some(format!("wrap it in `{} {{ … }}`", parents[0])),
                    );
                    return None;
                }
                if let Some((parent, allowed)) = parent
                    && !allowed.contains(&kind)
                {
                    let held: Vec<&str> = allowed.iter().map(|node| node.as_str()).collect();
                    self.report.on(
                        node,
                        format!("`{parent}` does not hold `{kind}`"),
                        Some(format!("`{parent}` holds {}", held.join(", "))),
                    );
                    return None;
                }
                self.node(node, kind, within).map(Item::Node)
            }
        }
    }

    fn unknown_node(&mut self, node: &KdlNode, name: &str) {
        if let Some(kind) = NodeName::parse_ignoring_case(name) {
            self.report.on(
                node,
                format!("`{name}` is not a catalog node: catalog nodes are PascalCase"),
                Some(format!("write `{kind}`")),
            );
        } else if let Some(directive) = DIRECTIVES
            .iter()
            .find(|directive| directive.eq_ignore_ascii_case(name))
        {
            self.report.on(
                node,
                format!("`{name}` is not a directive: directives are lowercase"),
                Some(format!("write `{directive}`")),
            );
        } else {
            let names: Vec<&str> = CATALOG.iter().map(|spec| spec.name.as_str()).collect();
            self.report.on(
                node,
                format!("`{name}` is not a catalog node"),
                Some(match nearest(name, &names) {
                    Some(near) => format!("did you mean `{near}`?"),
                    None => format!("a view draws {}", names.join(", ")),
                }),
            );
        }
    }

    fn node(&mut self, node: &KdlNode, kind: NodeName, within: &Within) -> Option<Node> {
        let spec = kind.spec();
        let parts = entries(node, self.report);
        let mut ok = true;
        let args = self.args(node, spec, &parts, &mut ok);
        let mut compiled = Node {
            name: kind,
            args,
            props: BTreeMap::new(),
            each: None,
            condition: None,
            children: Vec::new(),
            span: span_of(node),
        };
        for (prop, entry) in &parts.props {
            ok &= self.prop(&mut compiled, prop, entry);
        }
        let mut inner = within.clone();
        if compiled.each.is_some() {
            inner.each += 1;
            if inner.each > EACH_MAX {
                self.report.on(
                    node,
                    format!("`each` nests deeper than the {EACH_MAX} levels a view may"),
                    Some("give the data a flatter shape".to_owned()),
                );
                ok = false;
            }
            if let Some(template) = &within.template
                && let Some(facts) = self.templates.get_mut(template)
            {
                facts.each = facts.each.max(inner.each);
            }
        }
        self.declare_state(kind, &compiled, node);
        compiled.children = self.children(node, kind, spec, &inner, &mut ok);
        ok.then_some(compiled)
    }

    fn args(
        &mut self,
        node: &KdlNode,
        spec: &NodeSpec,
        parts: &Entries<'_>,
        ok: &mut bool,
    ) -> Vec<Binding> {
        let kind = spec.name;
        let mut args = Vec::new();
        for (index, arg) in parts.args.iter().enumerate() {
            let Some(arg_kind) = spec.args.get(index) else {
                self.report.on(
                    *arg,
                    format!("`{kind}` takes {}", count(spec.args.len(), "argument")),
                    Some(if spec.children.is_some() {
                        format!(
                            "put {} in its own node inside the block",
                            spelled(arg.value())
                        )
                    } else {
                        format!("remove {}", spelled(arg.value()))
                    }),
                );
                *ok = false;
                continue;
            };
            match self.bind(*arg_kind, arg, &format!("`{kind}`'s argument")) {
                Some(binding) => args.push(binding),
                None => *ok = false,
            }
        }
        if parts.args.len() < spec.required_args {
            *ok = false;
            if self.brace_for_a_string(node, spec) {
                return args;
            }
            self.report.on(
                node,
                format!(
                    "`{kind}` takes {}, and {} given",
                    count(spec.required_args, "argument"),
                    match parts.args.len() {
                        0 => "none is".to_owned(),
                        n => format!("{n} are"),
                    }
                ),
                Some(format!("write `{kind} \"…\"`")),
            );
        }
        args
    }

    /// `Text {id}`: a brace pasted from JSX, which KDL reads as a block
    /// holding a node named `id`. Says so, at the brace, and returns
    /// whether it did.
    fn brace_for_a_string(&mut self, node: &KdlNode, spec: &NodeSpec) -> bool {
        let Some(children) = node.children() else {
            return false;
        };
        if spec.children.is_some() || spec.args.first() != Some(&PropKind::Text) {
            return false;
        }
        let pasted = match children.nodes() {
            [only] if only.entries().is_empty() && only.children().is_none() => {
                Some(only.name().value().to_owned())
            }
            _ => None,
        };
        let brace = children.nodes().first().map_or(children.offset(), |first| {
            self.report.source()[..first.offset()]
                .rfind('{')
                .unwrap_or(first.offset())
        });
        let kind = spec.name;
        self.report.at(
            brace,
            format!("`{kind}` takes a string, and `{{ … }}` here is a block of nodes, not text"),
            Some(match pasted {
                Some(field) => format!("write `{kind} \"{{{field}}}\"`"),
                None => format!("write `{kind} \"…\"`"),
            }),
        );
        true
    }

    fn prop(&mut self, compiled: &mut Node, prop: &str, entry: &KdlEntry) -> bool {
        let kind = compiled.name;
        if prop == "each" {
            return self.each(compiled, entry);
        }
        if CONDITIONS.contains(&prop) {
            return self.condition(compiled, prop, entry);
        }
        let Some(prop_kind) = kind.prop(prop) else {
            let own: Vec<&str> = kind
                .spec()
                .props
                .iter()
                .chain(COMMON_PROPS)
                .map(|(name, _)| *name)
                .collect();
            let near = own.iter().find(|name| name.eq_ignore_ascii_case(prop));
            self.report.on(
                entry,
                format!("`{kind}` has no property `{prop}`"),
                Some(match (near, nearest(prop, &own)) {
                    (Some(name), _) => format!("properties are kebab-case: write `{name}=`"),
                    (None, Some(close)) => format!("did you mean `{close}=`?"),
                    (None, None) => accepts(&own),
                }),
            );
            return false;
        };
        let Some(binding) = self.bind(prop_kind, entry, &format!("`{prop}`")) else {
            return false;
        };
        if prop == "id"
            && !matches!(&binding, Binding::Literal(serde_json::Value::String(id)) if is_field_name(id))
        {
            self.report.on(
                entry,
                "an `id` is a name a view reads back as `{ui.<id>}`: letters, digits, `_` and `-`",
                None,
            );
            return false;
        }
        compiled.props.insert(prop.to_owned(), binding);
        true
    }

    fn each(&mut self, compiled: &mut Node, entry: &KdlEntry) -> bool {
        let path = self.identity(entry, "`each`").and_then(|text| {
            let path = Path::parse(&text);
            if path.is_none() {
                self.report.on(
                    entry,
                    format!("`each` names one list field, and `{text}` is not a field"),
                    Some("write `each=\"phases\"`".to_owned()),
                );
            }
            path
        });
        compiled.each = path;
        compiled.each.is_some()
    }

    fn condition(&mut self, compiled: &mut Node, prop: &str, entry: &KdlEntry) -> bool {
        if compiled.condition.is_some() {
            self.report.on(
                entry,
                "a node takes one of `when`, `unless`, `show` and `show-any`",
                Some("nest one node inside another for both".to_owned()),
            );
            return false;
        }
        let Some(text) = self.identity(entry, &format!("`{prop}`")) else {
            return false;
        };
        let parsed = match prop {
            "when" | "unless" => Path::parse(&text)
                .map(|path| {
                    if prop == "when" {
                        Condition::When(path)
                    } else {
                        Condition::Unless(path)
                    }
                })
                .ok_or_else(|| {
                    (
                        format!("`{prop}` names one field and tests whether it is present, and `{text}` is not a field"),
                        format!("write `{prop}=\"next\"`, and give the data the field to test"),
                    )
                }),
            "show" => show_pair(&text).map(|(id, value)| Condition::Show { id, value }),
            _ => text
                .split([',', ' '])
                .filter(|pair| !pair.is_empty())
                .map(show_pair)
                .collect::<Result<Vec<_>, _>>()
                .and_then(|pairs| {
                    if pairs.is_empty() {
                        Err(show_error(&text))
                    } else {
                        Ok(Condition::ShowAny(pairs))
                    }
                }),
        };
        match parsed {
            Ok(condition) => {
                let pairs: Vec<(String, String)> = match &condition {
                    Condition::Show { id, value } => vec![(id.clone(), value.clone())],
                    Condition::ShowAny(pairs) => pairs.clone(),
                    _ => Vec::new(),
                };
                for (id, value) in pairs {
                    self.shows.push((id, value, entry.offset()));
                }
                compiled.condition = Some(condition);
                true
            }
            Err((message, suggestion)) => {
                self.report.on(entry, message, Some(suggestion));
                false
            }
        }
    }

    /// A property that must be a literal string, as written.
    fn identity(&mut self, entry: &KdlEntry, what: &str) -> Option<String> {
        match self.bind(PropKind::Identity, entry, what) {
            Some(Binding::Literal(serde_json::Value::String(text))) => Some(text),
            _ => None,
        }
    }

    /// Binds one value to what `kind` asks for, reporting why it cannot.
    /// `what` names the value in a message.
    fn bind(&mut self, kind: PropKind, entry: &KdlEntry, what: &str) -> Option<Binding> {
        let value = entry.value();
        let result = match (kind, value) {
            (PropKind::Text, KdlValue::String(text)) => self.text(text, entry),
            (PropKind::Text, other) => Err((
                format!("{what} is text, a string, not {}", spelled(other)),
                Some(format!("write \"{other}\"")),
            )),
            (PropKind::Identity, KdlValue::String(text)) => match self.text(text, entry) {
                Ok(Binding::Literal(literal)) => Ok(Binding::Literal(literal)),
                Err(error) => Err(error),
                Ok(_) => Err((
                    format!(
                        "{what} must be a literal: what an extension is keyed by never depends on data"
                    ),
                    Some(
                        "write a fixed name, and move the varying part into `item=` or the data"
                            .to_owned(),
                    ),
                )),
            },
            (PropKind::Identity, other) => Err((
                format!("{what} is a name, a string, not {}", spelled(other)),
                Some(format!("write \"{other}\"")),
            )),
            (PropKind::Word(words), KdlValue::String(word)) if words.contains(&word.as_str()) => {
                Ok(Binding::Literal(serde_json::Value::String(word.clone())))
            }
            (PropKind::Word(words), other) => Err((
                format!("{what} is not {}", spelled(other)),
                Some(format!("write one of {}", words.join(", "))),
            )),
            (PropKind::Role, KdlValue::String(text)) => self.role(text, entry, what),
            (PropKind::Role, other) => Err((
                format!("{what} is a role, not {}", spelled(other)),
                Some(roles()),
            )),
            (PropKind::Count, KdlValue::Integer(number)) if *number >= 0 => Ok(literal(value)),
            (PropKind::Number, KdlValue::Integer(_) | KdlValue::Float(_)) => match literal(value) {
                Binding::Literal(serde_json::Value::Null) => {
                    Err((format!("{what} is not a finite number"), None))
                }
                number => Ok(number),
            },
            (PropKind::Flag, KdlValue::Bool(flag)) => {
                Ok(Binding::Literal(serde_json::Value::Bool(*flag)))
            }
            (PropKind::Count | PropKind::Number | PropKind::Flag, KdlValue::String(text)) => {
                self.lone(text, entry, kind, what)
            }
            (PropKind::Count | PropKind::Number | PropKind::Flag, other) => Err((
                format!("{what} is {}, not {}", noun(kind), spelled(other)),
                Some(match kind {
                    PropKind::Flag => "write `#true`, `#false` or a lone reference".to_owned(),
                    _ => "write a number or a lone reference such as \"{plans}\"".to_owned(),
                }),
            )),
            (PropKind::Value, KdlValue::String(text)) => self.text(text, entry),
            (PropKind::Value, other) => Ok(literal(other)),
        };
        match result {
            Ok(binding) => Some(binding),
            Err((message, suggestion)) => {
                self.report.on(entry, message, suggestion);
                None
            }
        }
    }

    fn text(&mut self, text: &str, entry: &KdlEntry) -> Result<Binding, (String, Option<String>)> {
        let parts = parse_text(text).map_err(|error| (error, None))?;
        for part in &parts {
            if let TextPart::Lookup { table, .. } | TextPart::Label { table, .. } = part {
                self.tables.push((table.clone(), entry.offset()));
            }
        }
        Ok(binding_of(parts))
    }

    fn lone(
        &mut self,
        text: &str,
        entry: &KdlEntry,
        kind: PropKind,
        what: &str,
    ) -> Result<Binding, (String, Option<String>)> {
        match self.text(text, entry)? {
            binding @ (Binding::Ref(_) | Binding::Lookup { .. } | Binding::Label { .. }) => {
                Ok(binding)
            }
            _ => Err((
                format!(
                    "{what} is {}: write it bare, or as one reference and nothing around it",
                    noun(kind)
                ),
                Some(match kind {
                    PropKind::Flag => "write `#true` or \"{field}\"".to_owned(),
                    _ => format!("write `{}`", text.trim()),
                }),
            )),
        }
    }

    fn role(
        &mut self,
        text: &str,
        entry: &KdlEntry,
        what: &str,
    ) -> Result<Binding, (String, Option<String>)> {
        match self.text(text, entry)? {
            Binding::Literal(serde_json::Value::String(name))
                if RoleName::parse(&name).is_some() =>
            {
                Ok(Binding::Literal(serde_json::Value::String(name)))
            }
            Binding::Literal(_) => {
                let names: Vec<&str> = RoleName::ALL.iter().map(|role| role.as_str()).collect();
                Err((
                    format!("{what} names no role: \"{text}\" is not one"),
                    Some(match nearest(text, &names) {
                        Some(near) => format!("did you mean `{near}`?"),
                        None => roles(),
                    }),
                ))
            }
            binding @ (Binding::Ref(_) | Binding::Lookup { .. } | Binding::Label { .. }) => {
                Ok(binding)
            }
            Binding::Text(_) => Err((
                format!("{what} is one role, or one reference to one, and nothing around it"),
                Some("map values to roles with a `lookup` and write \"{table:field}\"".to_owned()),
            )),
        }
    }

    fn children(
        &mut self,
        node: &KdlNode,
        kind: NodeName,
        spec: &NodeSpec,
        within: &Within,
        ok: &mut bool,
    ) -> Vec<Item> {
        let Some(block) = node.children() else {
            return Vec::new();
        };
        let Some(allowed) = spec.children else {
            // `Text {id}` was already answered as a pasted brace.
            let pasted_brace = node.entries().iter().all(|entry| entry.name().is_some())
                && spec.args.first() == Some(&PropKind::Text);
            if !pasted_brace {
                self.report.on(
                    block,
                    format!("`{kind}` holds no other nodes"),
                    Some("remove the `{ … }`, or wrap both in a `Row` or a `Stack`".to_owned()),
                );
            }
            *ok = false;
            return Vec::new();
        };
        let mut items = Vec::new();
        for child in block.nodes() {
            match self.item(child, within, Some((kind, allowed))) {
                Some(item) => items.push(item),
                None => *ok = false,
            }
        }
        items
    }

    fn declare_state(&mut self, kind: NodeName, compiled: &Node, node: &KdlNode) {
        let Some(Binding::Literal(serde_json::Value::String(id))) = compiled.props.get("id") else {
            return;
        };
        let values = (kind == NodeName::Tabs).then(|| {
            node.children()
                .map(KdlDocument::nodes)
                .unwrap_or_default()
                .iter()
                .filter_map(|tab| match tab.get("value") {
                    Some(KdlValue::String(value)) => Some(value.clone()),
                    _ => None,
                })
                .collect()
        });
        if self.states.insert(id.clone(), State { values }).is_some() {
            self.report.on(
                node,
                format!("`id=\"{id}\"` is declared twice in this view"),
                Some("give each its own id".to_owned()),
            );
        }
    }

    fn use_template(&mut self, node: &KdlNode, within: &Within) -> Option<Item> {
        let parts = entries(node, self.report);
        refuse_extra(node, &parts, 1, &[], false, self.report);
        let name = match parts.args.first().map(|arg| arg.value()) {
            Some(KdlValue::String(name)) => name.clone(),
            _ => {
                self.report.on(
                    node,
                    "`use` names a template",
                    Some("write `use \"name\"`".to_owned()),
                );
                return None;
            }
        };
        self.uses.push(UseSite {
            template: name.clone(),
            within: within.clone(),
            offset: node.offset(),
        });
        Some(Item::Use {
            template: name,
            span: span_of(node),
        })
    }

    fn template(&mut self, node: &KdlNode) {
        let parts = entries(node, self.report);
        refuse_extra(node, &parts, 1, &[], true, self.report);
        let Some(name) = self.declared_name(node, &parts, "template") else {
            return;
        };
        if self.templates.contains_key(&name) {
            self.report.on(
                node,
                format!("the template `{name}` is declared twice"),
                None,
            );
            return;
        }
        let Some(body) = node.children().filter(|body| !body.nodes().is_empty()) else {
            self.report.on(
                node,
                format!("the template `{name}` holds nothing"),
                Some(format!("write `template \"{name}\" {{ … }}`")),
            );
            return;
        };
        self.templates.insert(
            name.clone(),
            TemplateFacts {
                offset: node.offset(),
                each: 0,
            },
        );
        let within = Within {
            each: 0,
            template: Some(name.clone()),
        };
        let items = body
            .nodes()
            .iter()
            .filter_map(|child| self.item(child, &within, None))
            .collect();
        self.view.templates.insert(name, items);
    }

    fn declared_name(&mut self, node: &KdlNode, parts: &Entries<'_>, what: &str) -> Option<String> {
        match parts.args.first().map(|arg| arg.value()) {
            Some(KdlValue::String(name)) if is_field_name(name) => Some(name.clone()),
            Some(KdlValue::String(name)) => {
                self.report.on(
                    node,
                    format!("`{name}` is not a {what} name: letters, digits, `_` and `-`, starting with a letter"),
                    None,
                );
                None
            }
            _ => {
                self.report.on(
                    node,
                    format!("`{what}` takes its name"),
                    Some(format!("write `{what} \"name\"`")),
                );
                None
            }
        }
    }

    fn lookup(&mut self, node: &KdlNode) {
        let parts = entries(node, self.report);
        refuse_extra(node, &parts, 1, &["locale", "default"], true, self.report);
        let name = self.declared_name(node, &parts, "lookup");
        let mut lookup = Lookup::default();
        for (prop, entry) in &parts.props {
            let value = match entry.value() {
                KdlValue::String(value) => value.clone(),
                other => {
                    self.report.on(
                        *entry,
                        format!("`{prop}` takes a string, not {}", spelled(other)),
                        Some(format!("write `{prop}=\"{other}\"`")),
                    );
                    continue;
                }
            };
            match *prop {
                "locale" => lookup.locale = Some(value),
                "default" => lookup.default = Some(value),
                _ => {}
            }
        }
        for entry in node.children().map(KdlDocument::nodes).unwrap_or_default() {
            let key = entry.name().value().to_owned();
            let parts = entries(entry, self.report);
            refuse_extra(entry, &parts, 1, &[], false, self.report);
            match parts.args.first().map(|arg| arg.value()) {
                Some(KdlValue::String(value)) => {
                    if lookup.entries.insert(key.clone(), value.clone()).is_some() {
                        self.report.on(
                            entry,
                            format!("`{key}` is given twice in this table"),
                            Some("keep one".to_owned()),
                        );
                    }
                }
                _ => self.report.on(
                    entry,
                    format!("`{key}` maps to a string"),
                    Some(format!("write `{key} \"…\"`")),
                ),
            }
        }
        let Some(name) = name else {
            return;
        };
        lookup.name = name;
        let repeated = self
            .view
            .lookups
            .iter()
            .any(|other| other.name == lookup.name && other.locale == lookup.locale);
        if repeated {
            self.report.on(
                node,
                format!(
                    "the table `{}` is declared twice{}",
                    lookup.name,
                    lookup
                        .locale
                        .as_ref()
                        .map(|locale| format!(" for `{locale}`"))
                        .unwrap_or_default()
                ),
                Some("give each its own `locale=`".to_owned()),
            );
            return;
        }
        self.view.lookups.push(lookup);
    }

    /// The second pass: what needs the whole file.
    fn finish(&mut self) {
        let cycles = self.refuse_cycles();
        self.refuse_unknown_templates();
        self.refuse_deep_uses(&cycles);
        self.refuse_unknown_tables();
        self.refuse_unknown_shows();
    }

    fn refuse_unknown_templates(&mut self) {
        let known: Vec<&str> = self.templates.keys().map(String::as_str).collect();
        for site in &self.uses {
            if !self.templates.contains_key(&site.template) {
                self.report.at(
                    site.offset,
                    format!("there is no template `{}` in this view", site.template),
                    Some(match (known.is_empty(), nearest(&site.template, &known)) {
                        (true, _) => format!(
                            "declare `template \"{}\" {{ … }}` at the top of the view",
                            site.template
                        ),
                        (false, Some(near)) => format!("did you mean `{near}`?"),
                        (false, None) => format!("this view's templates: {}", known.join(", ")),
                    }),
                );
            }
        }
    }

    /// The templates each template uses.
    fn graph(&self) -> BTreeMap<&str, BTreeSet<&str>> {
        let mut graph: BTreeMap<&str, BTreeSet<&str>> = self
            .templates
            .keys()
            .map(|name| (name.as_str(), BTreeSet::new()))
            .collect();
        for site in &self.uses {
            if let Some(owner) = &site.within.template
                && self.templates.contains_key(&site.template)
            {
                graph
                    .entry(owner.as_str())
                    .or_default()
                    .insert(site.template.as_str());
            }
        }
        graph
    }

    /// Reports each cycle once, naming it, and returns the templates on one.
    fn refuse_cycles(&mut self) -> BTreeSet<String> {
        let graph = self.graph();
        let mut on_cycle = BTreeSet::new();
        let mut found = Vec::new();
        for &start in graph.keys() {
            if on_cycle.contains(start) {
                continue;
            }
            if let Some(cycle) = cycle_from(&graph, start) {
                on_cycle.extend(cycle.iter().map(|name| (*name).to_owned()));
                found.push(
                    cycle
                        .iter()
                        .map(|name| (*name).to_owned())
                        .collect::<Vec<_>>(),
                );
            }
        }
        for cycle in found {
            let offset = self.templates[&cycle[0]].offset;
            let named = cycle
                .iter()
                .chain(cycle.first())
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>();
            self.report.at(
                offset,
                format!("templates use each other in a cycle: {}", named.join(" → ")),
                Some(
                    "a template may use another only if that one does not lead back to it"
                        .to_owned(),
                ),
            );
        }
        on_cycle
    }

    fn refuse_deep_uses(&mut self, cycles: &BTreeSet<String>) {
        let graph = self.graph();
        let mut depths = BTreeMap::new();
        for name in self.templates.keys() {
            if !cycles.contains(name) {
                self.template_depth(name, &graph, &mut depths);
            }
        }
        let deep: Vec<(usize, String, usize)> = self
            .uses
            .iter()
            .filter(|site| site.within.template.is_none())
            .filter_map(|site| {
                let inner = depths.get(site.template.as_str())?;
                let total = site.within.each + inner;
                (total > EACH_MAX).then(|| (site.offset, site.template.clone(), total))
            })
            .collect();
        for (offset, template, total) in deep {
            self.report.at(
                offset,
                format!(
                    "`each` nests {total} levels deep through the template `{template}`, deeper than the {EACH_MAX} a view may"
                ),
                Some("give the data a flatter shape".to_owned()),
            );
        }
    }

    /// The deepest `each` a template reaches through the ones it uses.
    fn template_depth<'g>(
        &self,
        name: &'g str,
        graph: &BTreeMap<&'g str, BTreeSet<&'g str>>,
        depths: &mut BTreeMap<&'g str, usize>,
    ) -> usize {
        if let Some(depth) = depths.get(name) {
            return *depth;
        }
        // Marked before its uses are walked: a template that reaches a
        // cycle without being on it would otherwise walk that cycle
        // forever. The cycle itself is said by `refuse_cycles`.
        depths.insert(name, 0);
        let own = self.templates.get(name).map_or(0, |facts| facts.each);
        let through = self
            .uses
            .iter()
            .filter(|site| site.within.template.as_deref() == Some(name))
            .filter_map(|site| {
                let used = graph.get_key_value(site.template.as_str())?.0;
                Some(site.within.each + self.template_depth(used, graph, depths))
            })
            .max()
            .unwrap_or(0);
        let depth = own.max(through);
        depths.insert(name, depth);
        depth
    }

    fn refuse_unknown_tables(&mut self) {
        let declared: BTreeSet<&str> = self
            .view
            .lookups
            .iter()
            .map(|lookup| lookup.name.as_str())
            .collect();
        for (table, offset) in &self.tables {
            if !declared.contains(table.as_str()) {
                self.report.at(
                    *offset,
                    format!("there is no lookup table `{table}` in this view"),
                    Some(
                        match nearest(table, &declared.iter().copied().collect::<Vec<_>>()) {
                            Some(near) => format!("did you mean `{near}`?"),
                            None => format!("declare `lookup \"{table}\" {{ value \"…\"; }}`"),
                        },
                    ),
                );
            }
        }
    }

    fn refuse_unknown_shows(&mut self) {
        for (id, value, offset) in &self.shows {
            match self.states.get(id) {
                None => self.report.at(
                    *offset,
                    format!(
                        "no node in this view declares `id=\"{id}\"`, so `ui.{id}` is never set"
                    ),
                    Some("declare it on a `Tabs`, a `Toggle` or a `List`".to_owned()),
                ),
                Some(State {
                    values: Some(values),
                }) if !values.contains(value) => {
                    let listed: Vec<&str> = values.iter().map(String::as_str).collect();
                    self.report.at(
                        *offset,
                        format!("the tabs `{id}` have no tab `{value}`"),
                        Some(format!("its tabs: {}", listed.join(", "))),
                    );
                }
                Some(_) => {}
            }
        }
    }
}

/// The first cycle reachable from `start` that comes back to it.
fn cycle_from<'g>(
    graph: &BTreeMap<&'g str, BTreeSet<&'g str>>,
    start: &'g str,
) -> Option<Vec<&'g str>> {
    fn walk<'g>(
        graph: &BTreeMap<&'g str, BTreeSet<&'g str>>,
        start: &'g str,
        path: &mut Vec<&'g str>,
        seen: &mut BTreeSet<&'g str>,
    ) -> bool {
        let current = *path.last().expect("the path starts at `start`");
        for &next in graph.get(current).into_iter().flatten() {
            if next == start {
                return true;
            }
            if seen.insert(next) {
                path.push(next);
                if walk(graph, start, path, seen) {
                    return true;
                }
                path.pop();
            }
        }
        false
    }
    let mut path = vec![start];
    let mut seen = BTreeSet::from([start]);
    walk(graph, start, &mut path, &mut seen).then_some(path)
}

fn show_pair(text: &str) -> Result<(String, String), (String, String)> {
    let (left, value) = text.split_once('=').ok_or_else(|| show_error(text))?;
    let id = Path::parse(left)
        .filter(|path| path.scope == Scope::Ui && path.segments.len() == 1)
        .and_then(|path| path.segments.into_iter().next())
        .ok_or_else(|| show_error(text))?;
    let value = value.trim();
    if value.is_empty() || value.contains(['{', '}']) {
        return Err(show_error(text));
    }
    Ok((id, value.to_owned()))
}

fn show_error(text: &str) -> (String, String) {
    (
        format!("`{text}` does not compare a UI value with a literal"),
        "write `show=\"ui.tab=phases\"`, or `show-any=\"ui.tab=a,ui.tab=b\"`".to_owned(),
    )
}

fn span_of(node: &KdlNode) -> Span {
    Span {
        offset: node.span().offset(),
        len: node.span().len(),
    }
}

fn noun(kind: PropKind) -> &'static str {
    match kind {
        PropKind::Count => "a whole number",
        PropKind::Number => "a number",
        PropKind::Flag => "`#true` or `#false`",
        _ => "a value",
    }
}

fn roles() -> String {
    let names: Vec<&str> = RoleName::ALL.iter().map(|role| role.as_str()).collect();
    format!("a role is one of {}", names.join(", "))
}

fn literal(value: &KdlValue) -> Binding {
    use serde_json::Value;
    Binding::Literal(match value {
        KdlValue::String(text) => Value::String(text.clone()),
        KdlValue::Integer(number) => i64::try_from(*number).map(Value::from).unwrap_or_else(|_| {
            serde_json::Number::from_f64(*number as f64).map_or(Value::Null, Value::Number)
        }),
        KdlValue::Float(number) => {
            serde_json::Number::from_f64(*number).map_or(Value::Null, Value::Number)
        }
        KdlValue::Bool(flag) => Value::Bool(*flag),
        KdlValue::Null => Value::Null,
    })
}
