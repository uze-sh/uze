//! The binder: a compiled view, its data and the UI state, into a tree of
//! [`view::Node`]s, with what each interactive node means and the trace of
//! how it was reached.
//!
//! One walk over the compiled items does all three, so what is drawn, what
//! a click on it does and what the inspector says about it can never
//! disagree.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use uze_contract::binding::is_present;
use uze_contract::compiled::Node as Compiled;
use uze_contract::{Clean, CompiledView, Condition, Item, NodeName};

use super::media::{Held, Lookup, MediaKey, Playing};
use super::time::since_ms;
use super::trace::{Absence, TraceNode};
use super::value::{Env, Scopes, as_count, as_flag, as_number, as_role, path_text, scalar_text};
use crate::view::{self, Align, CellGlyph, CellRegion, ChartKind, NodeKind, Role, Segment, Tab};

/// The most nodes one tree holds. Past it the build stops and says so:
/// two nested `each` over a large answer would otherwise draw (and cost)
/// without bound (D28).
pub const NODE_CAP: usize = 2_000;

/// The most rows and columns a `Cells` grid draws, and regions it binds:
/// past them the rest is left out, as a log's lines are counted.
const CELLS_ROWS_MAX: usize = 200;
const CELLS_COLUMNS_MAX: usize = 400;
const REGIONS_MAX: usize = 1_000;

/// The rows an `Image` takes when it names no `height`, and the most a
/// picture or a sprite is given.
const IMAGE_ROWS: u16 = 8;
const MEDIA_ROWS_MAX: u16 = 64;

/// How deep `use` may nest. The compiler refuses a cycle; this keeps a
/// view compiled by something else from recursing without end.
const TEMPLATE_DEPTH: usize = 16;

/// What a gesture on a node means, by the node's id.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Act {
    /// A tab sets `key` to its value.
    Tabs {
        key: String,
        values: Vec<String>,
        active: usize,
    },
    /// Flipping sets `key` to the other of `true` and `false`.
    Toggle { key: String, on: bool },
    /// Pressing records `action` with the `item` it would receive.
    Button { action: String, item: Value },
    /// A click in a region of a `Cells` records its `action` with its
    /// `item`, by the region's index.
    Cells { regions: Vec<(String, Value)> },
    /// Choosing a row sets `key` to its index.
    List {
        key: String,
        rows: usize,
        selected: Option<usize>,
        /// The item each row was drawn from, `null` for a row no `each`
        /// repeated: what an action on the selection receives.
        items: Vec<Value>,
    },
    /// Stepping sets `key` to the next (or previous) of `values`.
    Select {
        key: String,
        values: Vec<String>,
        chosen: usize,
    },
    /// Typing sets `key`; Enter inside a form sends the form whose node
    /// is `form`.
    Field { key: String, form: Option<usize> },
    /// Sending records `action` with `item` and the values of `fields`,
    /// by each field's id and the key its value is held under.
    Form {
        key: String,
        action: String,
        item: Value,
        fields: Vec<(String, String)>,
    },
}

impl Act {
    /// The key focus names this input by, when it is one focus stops at.
    pub(super) fn input_key(&self) -> Option<&str> {
        match self {
            Self::Select { key, .. } | Self::Field { key, .. } | Self::Form { key, .. } => {
                Some(key)
            }
            _ => None,
        }
    }
}

/// The UI keys the engine keeps for itself: which input has focus, and
/// which field is being typed into. A `#` is no field name, so no view can
/// read either back as `ui.`.
pub(super) const FOCUS: &str = "#focus";
pub(super) const EDITING: &str = "#editing";

/// One build of a surface.
#[derive(Clone, Debug)]
pub(super) struct Built {
    pub root: view::Node,
    pub acts: BTreeMap<usize, Act>,
    pub items: Vec<TraceNode>,
    pub templates: BTreeMap<String, Vec<TraceNode>>,
    pub folded: bool,
    /// How many nodes the tree holds.
    pub count: usize,
    /// The pictures it named, held or not: what the next read is chosen
    /// from.
    pub media: BTreeSet<MediaKey>,
    /// The sprites it plays, by where they stand in the view.
    pub playing: BTreeMap<String, Playing>,
}

impl Built {
    /// A tree that is only something the engine has to say, such as that
    /// the data has not arrived.
    pub fn notice(view: &CompiledView, text: &str, role: Role) -> Self {
        Self {
            root: view::Node::new(
                0,
                NodeKind::Notice {
                    text: Clean::label(text).into_string(),
                    role,
                },
            ),
            acts: BTreeMap::new(),
            items: TraceNode::skeleton(&view.items),
            templates: skeletons(view),
            folded: false,
            count: 1,
            media: BTreeSet::new(),
            playing: BTreeMap::new(),
        }
    }
}

fn skeletons(view: &CompiledView) -> BTreeMap<String, Vec<TraceNode>> {
    view.templates
        .iter()
        .map(|(name, items)| (name.clone(), TraceNode::skeleton(items)))
        .collect()
}

/// The UI state a view reads: what the host holds, and for every `Tabs`
/// with an `id` the host holds nothing for, its first tab, so `show`
/// agrees with the tab drawn as active.
pub(super) fn effective_ui(
    view: &CompiledView,
    held: Option<&BTreeMap<String, String>>,
) -> BTreeMap<String, String> {
    let mut ui = held.cloned().unwrap_or_default();
    let mut defaults = Vec::new();
    for items in std::iter::once(&view.items).chain(view.templates.values()) {
        tab_defaults(items, &mut defaults);
    }
    for (id, value) in defaults {
        ui.entry(id).or_insert(value);
    }
    ui
}

fn tab_defaults(items: &[Item], out: &mut Vec<(String, String)>) {
    for item in items {
        let Item::Node(node) = item else { continue };
        // The first tab, as `tabs` draws it active with nothing held: not
        // the first that happens to name a value.
        if matches!(node.name, NodeName::Tabs | NodeName::Select)
            && let Some(id) = literal(node, "id")
            && let Some(Item::Node(first)) = node.children.first()
            && let Some(value) = tab_value(first)
        {
            out.push((id, value));
        }
        tab_defaults(&node.children, out);
    }
}

/// What a tab's choice is held as, when the view says it outright: its
/// `value`, else its label when that is a literal, as `make_tab` takes it.
pub(super) fn tab_value(tab: &Compiled) -> Option<String> {
    if !matches!(tab.name, NodeName::Tab | NodeName::Choice) {
        return None;
    }
    literal(tab, "value").or_else(|| match tab.args.first()? {
        uze_contract::Binding::Literal(value) => value
            .as_str()
            .map(|label| Clean::label(label).into_string()),
        _ => None,
    })
}

/// Every `id` a view declares, so a newer compile keeps only the state
/// whose ids still exist.
pub(super) fn declared_ids(view: &CompiledView) -> Vec<String> {
    fn walk(items: &[Item], out: &mut Vec<String>) {
        for item in items {
            let Item::Node(node) = item else { continue };
            out.extend(literal(node, "id"));
            walk(&node.children, out);
        }
    }
    let mut ids = Vec::new();
    for items in std::iter::once(&view.items).chain(view.templates.values()) {
        walk(items, &mut ids);
    }
    ids
}

/// An identity property: always a literal, checked by the compiler.
fn literal(node: &Compiled, prop: &str) -> Option<String> {
    match node.props.get(prop)? {
        uze_contract::Binding::Literal(value) => value.as_str().map(str::to_owned),
        _ => None,
    }
}

/// Binds `view` to `data` under `env`.
pub(super) fn build<'a>(
    view: &'a CompiledView,
    data: &'a Value,
    env: Env<'a>,
    lookup: Lookup<'a>,
) -> Built {
    let mut builder = Builder {
        env,
        lookup,
        media: BTreeSet::new(),
        playing: BTreeMap::new(),
        view,
        next_id: 0,
        count: 0,
        folded: false,
        acts: BTreeMap::new(),
        cursor: Vec::new(),
        depth: 0,
        templates: skeletons(view),
        forms: Vec::new(),
    };
    let mut items = TraceNode::skeleton(&view.items);
    let mut scopes = vec![data];
    let nodes = builder.expand(&view.items, &mut scopes, &mut items, make_node);
    let root = builder.root(nodes);
    Built {
        root,
        acts: builder.acts,
        items,
        templates: builder.templates,
        folded: builder.folded,
        count: builder.count,
        media: builder.media,
        playing: builder.playing,
    }
}

/// Turns one compiled node, in scope, into what its parent collects.
type Make<'a, T> = fn(&mut Builder<'a>, &'a Compiled, &mut Scopes<'a>, &mut TraceNode) -> Option<T>;

struct Builder<'a> {
    env: Env<'a>,
    lookup: Lookup<'a>,
    media: BTreeSet<MediaKey>,
    playing: BTreeMap<String, Playing>,
    view: &'a CompiledView,
    next_id: usize,
    count: usize,
    folded: bool,
    acts: BTreeMap<usize, Act>,
    /// Where in the compiled view the walk is, as child indexes: the key a
    /// `Tabs`, `Toggle` or `List` without an `id` keeps its state under.
    cursor: Vec<usize>,
    depth: usize,
    templates: BTreeMap<String, Vec<TraceNode>>,
    /// The forms being built, innermost last: each one's node id and the
    /// fields found in it so far, by id and key.
    forms: Vec<(usize, Vec<(String, String)>)>,
}

impl<'a> Builder<'a> {
    fn expand<T>(
        &mut self,
        items: &'a [Item],
        scopes: &mut Scopes<'a>,
        traces: &mut [TraceNode],
        make: Make<'a, T>,
    ) -> Vec<T> {
        let mut out = Vec::new();
        for (index, (item, trace)) in items.iter().zip(traces.iter_mut()).enumerate() {
            self.cursor.push(index);
            match item {
                Item::Node(node) => self.instances(node, scopes, trace, make, &mut out),
                Item::Use { template, .. } => {
                    self.use_template(template, scopes, trace, make, &mut out)
                }
            }
            self.cursor.pop();
        }
        out
    }

    /// A template's items, bound against the current item. Its trace is
    /// taken out of the map while it is walked, which an acyclic `use`
    /// graph never notices.
    fn use_template<T>(
        &mut self,
        template: &str,
        scopes: &mut Scopes<'a>,
        trace: &mut TraceNode,
        make: Make<'a, T>,
        out: &mut Vec<T>,
    ) {
        trace.reached += 1;
        let Some(items) = self.view.templates.get(template) else {
            trace.absent(Absence::UnknownTemplate);
            return;
        };
        if self.depth >= TEMPLATE_DEPTH {
            trace.absent(Absence::UnknownTemplate);
            return;
        }
        let mut traces = self.templates.remove(template).unwrap_or_default();
        self.depth += 1;
        let made = self.expand(items, scopes, &mut traces, make);
        self.depth -= 1;
        self.templates.insert(template.to_owned(), traces);
        trace.drawn += u32::from(!made.is_empty());
        out.extend(made);
    }

    /// The node once, or once per item of its `each`. A `List` or a
    /// `Table` is drawn once and repeats its rows instead.
    fn instances<T>(
        &mut self,
        node: &'a Compiled,
        scopes: &mut Scopes<'a>,
        trace: &mut TraceNode,
        make: Make<'a, T>,
        out: &mut Vec<T>,
    ) {
        let Some(each) = node.each.as_ref().filter(|_| !repeats_rows(node.name)) else {
            self.instance(node, scopes, trace, make, out);
            return;
        };
        match self.list(scopes, each) {
            Ok(elements) => {
                for element in elements {
                    scopes.push(element);
                    self.instance(node, scopes, trace, make, out);
                    scopes.pop();
                }
            }
            Err(reason) => {
                trace.reached += 1;
                trace.absent(reason);
            }
        }
    }

    fn instance<T>(
        &mut self,
        node: &'a Compiled,
        scopes: &mut Scopes<'a>,
        trace: &mut TraceNode,
        make: Make<'a, T>,
        out: &mut Vec<T>,
    ) {
        trace.reached += 1;
        if let Some(reason) = self.excluded(node, scopes) {
            trace.absent(reason);
            return;
        }
        if self.count >= NODE_CAP {
            self.folded = true;
            trace.absent(Absence::CapReached);
            return;
        }
        self.count += 1;
        if let Some(made) = make(self, node, scopes, trace) {
            trace.drawn += 1;
            out.push(made);
        }
    }

    /// The items of the list `each` names, or why there are none.
    fn list(
        &self,
        scopes: &[&'a Value],
        each: &uze_contract::Path,
    ) -> Result<&'a [Value], Absence> {
        let field = || path_text(each);
        match self.env.resolve(scopes, each) {
            None => Err(Absence::EachMissing { field: field() }),
            Some(Cow::Borrowed(Value::Array(elements))) if elements.is_empty() => {
                Err(Absence::EachEmpty { field: field() })
            }
            Some(Cow::Borrowed(Value::Array(elements))) => Ok(elements),
            Some(_) => Err(Absence::EachNotList { field: field() }),
        }
    }

    /// Why `node` is not drawn here, if it is not.
    fn excluded(&self, node: &Compiled, scopes: &[&'a Value]) -> Option<Absence> {
        if let Some(condition) = &node.condition
            && let Some(reason) = self.unmet(condition, scopes)
        {
            return Some(reason);
        }
        node.args
            .iter()
            .chain(node.props.values())
            .find_map(|binding| self.env.missing(scopes, binding))
            .map(|field| Absence::Missing { field })
    }

    fn unmet(&self, condition: &Condition, scopes: &[&'a Value]) -> Option<Absence> {
        match condition {
            Condition::When(path) => {
                let value = self.env.resolve(scopes, path);
                (!is_present(value.as_deref())).then(|| Absence::When {
                    field: path_text(path),
                    missing: value.is_none(),
                })
            }
            Condition::Unless(path) => {
                is_present(self.env.resolve(scopes, path).as_deref()).then(|| Absence::Unless {
                    field: path_text(path),
                })
            }
            Condition::Show { id, value } => {
                let actual = self.ui(id);
                (actual != Some(value.as_str())).then(|| Absence::Show {
                    id: id.clone(),
                    value: value.clone(),
                    actual: actual.map(str::to_owned),
                })
            }
            Condition::ShowAny(pairs) => (!pairs
                .iter()
                .any(|(id, value)| self.ui(id) == Some(value.as_str())))
            .then_some(Absence::ShowAny),
        }
    }

    /// A UI value, named with or without its `ui.` prefix.
    fn ui(&self, id: &str) -> Option<&str> {
        let id = id.strip_prefix("ui.").unwrap_or(id);
        self.env.ui.get(id).map(String::as_str)
    }

    /// Whether the input kept under `key` has focus.
    fn focused(&self, key: &str) -> bool {
        self.env.ui.get(FOCUS).is_some_and(|focus| focus == key)
    }

    /// Notes an input with an `id` as a field of the innermost form being
    /// built, and answers that form's node.
    fn in_form(&mut self, node: &Compiled, key: &str) -> Option<usize> {
        let (form, fields) = self.forms.last_mut()?;
        if let Some(id) = literal(node, "id") {
            fields.push((id, key.to_owned()));
        }
        Some(*form)
    }

    fn id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Where a stateful node keeps its state: its `id`, or where it sits in
    /// the compiled view when it has none. A `#` key is no field name, so
    /// no reference can read it back as `ui.`.
    fn key(&self, node: &Compiled) -> String {
        literal(node, "id").unwrap_or_else(|| {
            let at: Vec<String> = self.cursor.iter().map(usize::to_string).collect();
            format!("#{}", at.join("."))
        })
    }

    fn prop(&self, node: &'a Compiled, scopes: &[&'a Value], name: &str) -> Option<Cow<'a, Value>> {
        self.env.bind(scopes, node.props.get(name)?)
    }

    fn arg(
        &self,
        node: &'a Compiled,
        scopes: &[&'a Value],
        index: usize,
    ) -> Option<Cow<'a, Value>> {
        self.env.bind(scopes, node.args.get(index)?)
    }

    fn label(&self, node: &'a Compiled, scopes: &[&'a Value]) -> String {
        self.clean_label(node, scopes).into_string()
    }

    fn clean_label(&self, node: &'a Compiled, scopes: &[&'a Value]) -> Clean {
        let text = self.arg(node, scopes, 0).map(|value| scalar_text(&value));
        Clean::label(&text.unwrap_or_default())
    }

    /// A `Text` that wraps is a paragraph, not a label: it is bounded as a
    /// block of text rather than cut at a label's length, and its breaks
    /// read as spaces, since the host wraps it to the room it has.
    fn paragraph(&self, node: &'a Compiled, scopes: &[&'a Value]) -> Clean {
        let text = self.arg(node, scopes, 0).map(|value| scalar_text(&value));
        Clean::text(&text.unwrap_or_default()).one_line()
    }

    /// A block of text that keeps its lines: Markdown or code, bounded as
    /// a block and cleaned like every other string.
    fn block(&self, node: &'a Compiled, scopes: &[&'a Value]) -> String {
        let text = self.arg(node, scopes, 0).map(|value| scalar_text(&value));
        Clean::text(&text.unwrap_or_default()).into_string()
    }

    fn text_prop(&self, node: &'a Compiled, scopes: &[&'a Value], name: &str) -> Option<String> {
        self.prop(node, scopes, name)
            .map(|value| Clean::label(&scalar_text(&value)).into_string())
    }

    fn count_prop(&self, node: &'a Compiled, scopes: &[&'a Value], name: &str) -> u16 {
        self.prop(node, scopes, name)
            .and_then(|value| as_count(&value))
            .unwrap_or(0)
    }

    fn flag_prop(&self, node: &'a Compiled, scopes: &[&'a Value], name: &str) -> bool {
        self.prop(node, scopes, name)
            .is_some_and(|value| as_flag(&value))
    }

    /// A word property's value, as the view or the data spells it.
    fn word_prop(&self, node: &'a Compiled, scopes: &[&'a Value], name: &str) -> Option<String> {
        self.prop(node, scopes, name)
            .and_then(|value| value.as_str().map(str::to_owned))
    }

    /// A role property other than `role`, when it is given.
    fn named_role(&self, node: &'a Compiled, scopes: &[&'a Value], name: &str) -> Option<Role> {
        self.prop(node, scopes, name).map(|value| as_role(&value))
    }

    fn role_prop(&self, node: &'a Compiled, scopes: &[&'a Value]) -> Role {
        self.prop(node, scopes, "role")
            .map(|value| as_role(&value))
            .unwrap_or_default()
    }

    /// A `List`'s or `Table`'s rows: its children, once per item of its
    /// `each` when it has one. An item whose children make more than one
    /// node is one row of them stacked, so a row's index is its item's.
    fn rows(
        &mut self,
        node: &'a Compiled,
        scopes: &mut Scopes<'a>,
        trace: &mut TraceNode,
    ) -> (Vec<view::Node>, Vec<Value>) {
        let Some(each) = &node.each else {
            let rows = self.expand(&node.children, scopes, &mut trace.children, make_node);
            let items = vec![Value::Null; rows.len()];
            return (rows, items);
        };
        let elements = match self.list(scopes, each) {
            Ok(elements) => elements,
            Err(reason) => {
                trace.absent(reason);
                return (Vec::new(), Vec::new());
            }
        };
        let filter = literal(node, "filter")
            .and_then(|named| self.ui(&named).map(|typed| typed.trim().to_lowercase()))
            .filter(|typed| !typed.is_empty());
        let mut rows = Vec::new();
        let mut items = Vec::new();
        for element in elements {
            if let Some(typed) = &filter
                && !holds_text(element, typed, FILTER_DEPTH)
            {
                continue;
            }
            scopes.push(element);
            let mut made = self.expand(&node.children, scopes, &mut trace.children, make_node);
            scopes.pop();
            match made.len() {
                0 => continue,
                1 => rows.extend(made.pop()),
                _ => {
                    let mut row = view::Node::new(self.id(), NodeKind::Stack { gap: 0 });
                    row.children = made;
                    rows.push(row);
                }
            }
            items.push(element.clone());
        }
        (rows, items)
    }

    /// The tree's root: the one top-level `Stack` as it is, anything else
    /// stacked, with the fold notice last when the cap was reached.
    fn root(&mut self, mut nodes: Vec<view::Node>) -> view::Node {
        let mut root = match nodes.len() {
            1 if matches!(nodes[0].kind, NodeKind::Stack { .. }) => nodes.remove(0),
            _ => {
                let mut stack = view::Node::new(self.id(), NodeKind::Stack { gap: 0 });
                stack.children = nodes;
                stack
            }
        };
        if self.folded {
            let text = format!("Folded: this view has more than {NODE_CAP} nodes");
            root.children.push(view::Node::new(
                self.id(),
                NodeKind::Notice {
                    text,
                    role: Role::Warning,
                },
            ));
        }
        root
    }
}

/// How deep a list's `filter` looks into a row's item for its text.
const FILTER_DEPTH: usize = 3;

/// Whether some text or number in `value`, `depth` levels down at most,
/// holds `typed` (already lowercase), ignoring case: what a list's
/// `filter` keeps a row by.
fn holds_text(value: &Value, typed: &str, depth: usize) -> bool {
    match value {
        Value::String(text) => text.to_lowercase().contains(typed),
        Value::Number(number) => number.to_string().contains(typed),
        Value::Array(values) if depth > 0 => values
            .iter()
            .any(|value| holds_text(value, typed, depth - 1)),
        Value::Object(fields) if depth > 0 => fields
            .values()
            .any(|value| holds_text(value, typed, depth - 1)),
        _ => false,
    }
}

/// Whether `each` on this node repeats its rows rather than the node.
fn repeats_rows(name: NodeName) -> bool {
    matches!(name, NodeName::List | NodeName::Table)
}

fn make_node<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &mut Scopes<'a>,
    trace: &mut TraceNode,
) -> Option<view::Node> {
    if matches!(
        node.name,
        NodeName::Mark | NodeName::Segment | NodeName::Tab | NodeName::Entry | NodeName::Choice
    ) {
        trace.absent(Absence::Misplaced);
        return None;
    }
    let id = builder.id();
    let mut children = Vec::new();
    let kind = match node.name {
        NodeName::Stack => {
            children = builder.expand(&node.children, scopes, &mut trace.children, make_node);
            NodeKind::Stack {
                gap: builder.count_prop(node, scopes, "gap"),
            }
        }
        NodeName::Row => {
            children = builder.expand(&node.children, scopes, &mut trace.children, make_node);
            NodeKind::Row {
                gap: builder.count_prop(node, scopes, "gap"),
                align: align(builder.prop(node, scopes, "align").as_deref()),
            }
        }
        NodeName::List => {
            let items;
            (children, items) = builder.rows(node, scopes, trace);
            list(builder, node, scopes, id, items)
        }
        NodeName::Table => {
            (children, _) = builder.rows(node, scopes, trace);
            NodeKind::Table {
                columns: (0..node.args.len())
                    .map(|at| column(builder, node, scopes, at))
                    .collect(),
                empty: builder.text_prop(node, scopes, "empty"),
            }
        }
        NodeName::Marks => {
            NodeKind::Marks(builder.expand(&node.children, scopes, &mut trace.children, make_mark))
        }
        NodeName::Progress => progress(builder, node, scopes, trace),
        NodeName::Tabs => tabs(builder, node, scopes, trace, id),
        NodeName::Select => select(builder, node, scopes, trace, id),
        NodeName::Form => {
            builder.forms.push((id, Vec::new()));
            children = builder.expand(&node.children, scopes, &mut trace.children, make_node);
            let fields = builder
                .forms
                .pop()
                .map(|(_, fields)| fields)
                .unwrap_or_default();
            form(builder, node, scopes, id, fields)
        }
        NodeName::Details => {
            children = builder.expand(&node.children, scopes, &mut trace.children, make_entry);
            NodeKind::Details
        }
        NodeName::Box => {
            children = builder.expand(&node.children, scopes, &mut trace.children, make_node);
            box_layout(builder, node, scopes)
        }
        NodeName::Grid => {
            children = builder.expand(&node.children, scopes, &mut trace.children, make_node);
            NodeKind::Grid {
                gap: builder.count_prop(node, scopes, "gap"),
            }
        }
        _ => leaf(builder, node, scopes, id),
    };
    let mut built = view::Node::new(id, kind);
    built.grow = builder.flag_prop(node, scopes, "grow");
    built.min_width = builder.count_prop(node, scopes, "min-width");
    built.children = children;
    Some(built)
}

/// One pair of a `Details`; anything else there is misplaced.
fn make_entry<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &mut Scopes<'a>,
    trace: &mut TraceNode,
) -> Option<view::Node> {
    if node.name != NodeName::Entry {
        trace.absent(Absence::Misplaced);
        return None;
    }
    let id = builder.id();
    Some(view::Node::new(id, leaf(builder, node, scopes, id)))
}

fn leaf<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &Scopes<'a>,
    id: usize,
) -> NodeKind {
    match node.name {
        NodeName::Text => {
            let truncate = builder.flag_prop(node, scopes, "truncate");
            let said = match truncate {
                true => builder.clean_label(node, scopes),
                false => builder.paragraph(node, scopes),
            };
            let mut span = view::Span::clean(said, builder.role_prop(node, scopes));
            span.bold = builder.flag_prop(node, scopes, "strong");
            NodeKind::Text {
                spans: vec![span],
                truncate,
            }
        }
        NodeName::Badge => NodeKind::Badge {
            text: builder.label(node, scopes),
            role: builder.role_prop(node, scopes),
        },
        NodeName::Pips => NodeKind::Pips {
            done: builder.count_prop(node, scopes, "done"),
            total: builder.count_prop(node, scopes, "total"),
        },
        NodeName::Toggle => {
            let key = builder.key(node);
            let on = match builder.env.ui.get(&key) {
                Some(held) => held == "true",
                None => builder.flag_prop(node, scopes, "on"),
            };
            builder.acts.insert(id, Act::Toggle { key, on });
            NodeKind::Toggle {
                label: builder.label(node, scopes),
                on,
            }
        }
        NodeName::Button => {
            let action = literal(node, "action").unwrap_or_default();
            let item = builder
                .prop(node, scopes, "item")
                .map(Cow::into_owned)
                .unwrap_or(Value::Null);
            builder.acts.insert(id, Act::Button { action, item });
            NodeKind::Button {
                label: builder.label(node, scopes),
            }
        }
        NodeName::Activity => NodeKind::Activity {
            busy: builder.flag_prop(node, scopes, "busy"),
        },
        NodeName::Elapsed => NodeKind::Elapsed {
            since_ms: builder
                .prop(node, scopes, "since")
                .and_then(|value| since_ms(&value)),
            role: builder.role_prop(node, scopes),
        },
        NodeName::Log => {
            let lines = builder
                .prop(node, scopes, "lines")
                .map(|value| log_lines(&value))
                .unwrap_or_default();
            // Each line is a row drawn, so it counts against the cap as a
            // node would: a log under an `each` cannot multiply a large
            // list past what one tree may hold.
            builder.count = builder.count.saturating_add(lines.len().saturating_sub(1));
            NodeKind::Log { lines }
        }
        NodeName::Rule => NodeKind::Rule {
            label: Some(builder.label(node, scopes)).filter(|label| !label.is_empty()),
            role: builder.role_prop(node, scopes),
        },
        NodeName::Entry => NodeKind::Entry {
            key: builder.label(node, scopes),
            value: builder
                .arg(node, scopes, 1)
                .map(|value| Clean::label(&scalar_text(&value)).into_string())
                .unwrap_or_default(),
            role: builder.role_prop(node, scopes),
        },
        NodeName::Markdown => NodeKind::Markdown {
            text: builder.block(node, scopes),
        },
        NodeName::Code => NodeKind::Code {
            text: builder.block(node, scopes),
            language: builder
                .text_prop(node, scopes, "language")
                .unwrap_or_default(),
        },
        NodeName::Gauge => gauge(builder, node, scopes),
        NodeName::Chart => chart(builder, node, scopes),
        NodeName::Stat => NodeKind::Stat {
            value: builder.label(node, scopes),
            label: builder
                .arg(node, scopes, 1)
                .map(|value| Clean::label(&scalar_text(&value)).into_string())
                .unwrap_or_default(),
            delta: builder
                .text_prop(node, scopes, "delta")
                .filter(|delta| !delta.is_empty()),
            role: builder.role_prop(node, scopes),
        },
        NodeName::Kbd => NodeKind::Kbd {
            key: builder.label(node, scopes),
        },
        NodeName::Cells => cells(builder, node, scopes, id),
        NodeName::Field => field(builder, node, scopes, id),
        NodeName::Image => image(builder, node, scopes),
        NodeName::Sprite => sprite(builder, node, scopes, id),
        // Containers are built by `make_node`, and parts of a container by
        // their container; neither reaches here.
        _ => NodeKind::Stack { gap: 0 },
    }
}

/// A grid of characters in roles, and the regions of it that run an
/// action. A role it does not know, a colour value among them, is the
/// default one: a declared extension never sends a colour.
fn cells<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &Scopes<'a>,
    id: usize,
) -> NodeKind {
    let rows = builder
        .prop(node, scopes, "rows")
        .map(|value| cell_rows(&value))
        .unwrap_or_default();
    // Each row is drawn, so it counts against the cap as a log's lines do.
    builder.count = builder.count.saturating_add(rows.len().saturating_sub(1));
    let mut regions = Vec::new();
    let mut meant = Vec::new();
    if let Some(Value::Array(declared)) = builder.prop(node, scopes, "regions").as_deref() {
        for region in declared.iter().take(REGIONS_MAX) {
            let side = |field: &str| region.get(field).and_then(as_count).unwrap_or(0);
            let Some(action) = region.get("action").and_then(Value::as_str) else {
                continue;
            };
            let (width, height) = (side("w"), side("h"));
            if width == 0 || height == 0 {
                continue;
            }
            regions.push(CellRegion {
                x: side("x"),
                y: side("y"),
                width,
                height,
            });
            meant.push((
                action.to_owned(),
                region.get("item").cloned().unwrap_or(Value::Null),
            ));
        }
    }
    builder.acts.insert(id, Act::Cells { regions: meant });
    NodeKind::Cells { rows, regions }
}

/// `rows`, each a string or a list of `{ch, role}` (or of strings).
fn cell_rows(value: &Value) -> Vec<Vec<CellGlyph>> {
    let Value::Array(rows) = value else {
        return Vec::new();
    };
    let cell = |cell: &Value| match cell {
        Value::String(text) => glyph(text.chars().next(), Role::Default),
        Value::Object(_) => glyph(
            cell.get("ch")
                .and_then(Value::as_str)
                .and_then(|text| text.chars().next()),
            cell.get("role").map(as_role).unwrap_or_default(),
        ),
        _ => glyph(None, Role::Default),
    };
    rows.iter()
        .take(CELLS_ROWS_MAX)
        .map(|row| match row {
            Value::String(text) => text
                .chars()
                .take(CELLS_COLUMNS_MAX)
                .map(|ch| glyph(Some(ch), Role::Default))
                .collect(),
            Value::Array(cells) => cells.iter().take(CELLS_COLUMNS_MAX).map(cell).collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// One cell: a character one column wide, or a blank. A control
/// character would reach the terminal as a byte, and a wide one would
/// push every cell after it out of its column.
fn glyph(ch: Option<char>, role: Role) -> CellGlyph {
    let ch = ch
        .filter(|ch| !ch.is_control() && unicode_width::UnicodeWidthChar::width(*ch) == Some(1))
        .unwrap_or(' ');
    CellGlyph { ch, role }
}

/// A picture as the shelf holds it, named for the next read when it holds
/// none.
fn image<'a>(builder: &mut Builder<'a>, node: &'a Compiled, scopes: &Scopes<'a>) -> NodeKind {
    let name = builder
        .arg(node, scopes, 0)
        .map(|value| scalar_text(&value))
        .unwrap_or_default();
    let height = match builder.count_prop(node, scopes, "height") {
        0 => IMAGE_ROWS,
        rows => rows.min(MEDIA_ROWS_MAX),
    };
    let key = MediaKey::Image(name);
    let (picture, refused) = match builder.lookup.shelf.get(&key) {
        Some(Held::Picture(picture)) => (Some(picture.clone()), None),
        Some(Held::Refused(why)) => (None, Some(Clean::label(why).into_string())),
        _ => (None, None),
    };
    builder.media.insert(key);
    NodeKind::Image {
        picture,
        alt: builder.text_prop(node, scopes, "alt").unwrap_or_default(),
        refused,
        height,
    }
}

/// A declared sprite playing the state `state` names, or its fallback,
/// from when the build first saw it play that state.
fn sprite<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &Scopes<'a>,
    id: usize,
) -> NodeKind {
    let named = builder
        .arg(node, scopes, 0)
        .map(|value| scalar_text(&value))
        .unwrap_or_default();
    let Some(decl) = builder
        .lookup
        .sprites
        .iter()
        .find(|sprite| sprite.id == named)
    else {
        return NodeKind::Sprite {
            frames: Vec::new(),
            fps: 0,
            looped: false,
            since: None,
            height: 1,
            refused: Some(format!(
                "it declares no sprite `{}`",
                Clean::label(&named).as_str()
            )),
        };
    };
    let asked = builder.text_prop(node, scopes, "state").unwrap_or_default();
    let state = decl
        .states
        .iter()
        .find(|state| state.name == asked)
        .or_else(|| decl.states.iter().find(|state| state.name == decl.fallback));
    let height = match builder.count_prop(node, scopes, "height") {
        0 => u16::try_from(decl.frame.1.div_ceil(2))
            .unwrap_or(u16::MAX)
            .clamp(1, IMAGE_ROWS),
        rows => rows.min(MEDIA_ROWS_MAX),
    };
    let key = MediaKey::Sheet(decl.id.clone());
    let (sheet, refused): (&[view::Picture], _) = match builder.lookup.shelf.get(&key) {
        Some(Held::Frames(frames)) => (frames, None),
        Some(Held::Refused(why)) => (&[], Some(Clean::label(why).into_string())),
        _ => (&[], None),
    };
    let frames: Vec<view::Picture> = state
        .map(|state| {
            state
                .frames
                .iter()
                .filter_map(|frame| sheet.get(usize::try_from(*frame).ok()?).cloned())
                .collect()
        })
        .unwrap_or_default();
    builder.media.insert(key);
    let playing = state.map(|state| state.name.clone()).unwrap_or_default();
    let at = format!("{}#{id}", builder.key(node));
    let since = match builder.lookup.before.and_then(|before| before.get(&at)) {
        Some(before) if before.state == playing => before.since,
        _ => builder.lookup.now,
    };
    builder.playing.insert(
        at,
        Playing {
            state: playing,
            since,
        },
    );
    NodeKind::Sprite {
        frames,
        fps: state.map_or(0, |state| u16::try_from(state.fps).unwrap_or(u16::MAX)),
        looped: state.is_some_and(|state| state.looped),
        since,
        height,
        refused,
    }
}

fn list<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &Scopes<'a>,
    id: usize,
    items: Vec<Value>,
) -> NodeKind {
    let rows = items.len();
    let key = builder.key(node);
    let selected = builder
        .env
        .ui
        .get(&key)
        .and_then(|held| held.parse::<usize>().ok())
        .filter(|row| *row < rows)
        // A picker is for choosing: it opens on its first row, so its keys
        // act at once.
        .or_else(|| (rows > 0 && builder.env.context["slot"] == "overlay").then_some(0));
    builder.acts.insert(
        id,
        Act::List {
            key,
            rows,
            selected,
            items,
        },
    );
    NodeKind::List {
        selected,
        empty: builder.text_prop(node, scopes, "empty"),
    }
}

fn column<'a>(builder: &Builder<'a>, node: &'a Compiled, scopes: &Scopes<'a>, at: usize) -> String {
    let text = builder
        .arg(node, scopes, at)
        .map(|value| scalar_text(&value));
    Clean::label(&text.unwrap_or_default()).into_string()
}

fn progress<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &mut Scopes<'a>,
    trace: &mut TraceNode,
) -> NodeKind {
    let mut segments = builder.expand(&node.children, scopes, &mut trace.children, make_segment);
    if segments.is_empty()
        && let Some(ratio) = builder
            .prop(node, scopes, "ratio")
            .and_then(|value| as_number(&value))
    {
        segments.push(Segment {
            weight: 1,
            filled: thousandths(ratio),
            role: builder.role_prop(node, scopes),
        });
    }
    NodeKind::Progress {
        segments,
        line: builder.word_prop(node, scopes, "style").as_deref() == Some("line"),
    }
}

/// A `Box`'s layout, every value resolved: a size or a role the data names
/// that is not one is left out, so the box is drawn as if it named none.
fn box_layout<'a>(builder: &Builder<'a>, node: &'a Compiled, scopes: &[&'a Value]) -> NodeKind {
    let padding = builder.count_prop(node, scopes, "padding");
    let side = |name: &str| match builder.prop(node, scopes, name) {
        Some(_) => builder.count_prop(node, scopes, name),
        None => padding,
    };
    // An empty ground is none: a lookup's default names no ground for the
    // rows it leaves bare.
    let ground = builder
        .prop(node, scopes, "ground")
        .filter(|value| !matches!(&**value, Value::Null) && value.as_str() != Some(""))
        .map(|value| as_role(&value))
        .map(|role| view::Ground {
            role,
            soft: builder.word_prop(node, scopes, "tone").as_deref() == Some("soft"),
        });
    let border = match builder.word_prop(node, scopes, "border").as_deref() {
        Some("line") => Some(view::BorderStyle::Line),
        Some("round") => Some(view::BorderStyle::Round),
        Some("heavy") => Some(view::BorderStyle::Heavy),
        _ => None,
    }
    .map(|style| view::Border {
        style,
        role: builder
            .named_role(node, scopes, "border-role")
            .unwrap_or(Role::Faint),
    });
    NodeKind::Box(view::BoxLayout {
        row: builder.word_prop(node, scopes, "direction").as_deref() == Some("row"),
        wrap: builder.flag_prop(node, scopes, "wrap"),
        gap: builder.count_prop(node, scopes, "gap"),
        width: builder
            .prop(node, scopes, "width")
            .and_then(|value| uze_contract::catalog::Size::parse(&value)),
        height: builder
            .prop(node, scopes, "height")
            .and_then(|value| as_count(&value)),
        pad_x: side("padding-x"),
        pad_y: side("padding-y"),
        justify: match builder.word_prop(node, scopes, "justify").as_deref() {
            Some("center") => view::Justify::Center,
            Some("end") => view::Justify::End,
            Some("between") => view::Justify::Between,
            _ => view::Justify::Start,
        },
        ground,
        border,
    })
}

fn tabs<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &mut Scopes<'a>,
    trace: &mut TraceNode,
    id: usize,
) -> NodeKind {
    let declared = builder.expand(&node.children, scopes, &mut trace.children, make_tab);
    let key = builder.key(node);
    let held = builder.env.ui.get(&key);
    let active = declared
        .iter()
        .position(|(_, value)| Some(value) == held)
        .unwrap_or(0);
    let tabs = declared
        .iter()
        .enumerate()
        .map(|(at, (label, _))| Tab {
            label: label.clone(),
            active: at == active,
        })
        .collect();
    let values = declared.into_iter().map(|(_, value)| value).collect();
    builder.acts.insert(
        id,
        Act::Tabs {
            key,
            values,
            active,
        },
    );
    NodeKind::Tabs { tabs }
}

fn make_mark<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &mut Scopes<'a>,
    trace: &mut TraceNode,
) -> Option<Role> {
    if node.name != NodeName::Mark {
        trace.absent(Absence::Misplaced);
        return None;
    }
    Some(builder.role_prop(node, scopes))
}

fn make_segment<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &mut Scopes<'a>,
    trace: &mut TraceNode,
) -> Option<Segment> {
    if node.name != NodeName::Segment {
        trace.absent(Absence::Misplaced);
        return None;
    }
    let number = |name| {
        builder
            .prop(node, scopes, name)
            .and_then(|value| as_number(&value))
    };
    let weight = number("weight")
        .unwrap_or(1.0)
        .round()
        .clamp(0.0, f64::from(u32::MAX)) as u32;
    Some(Segment {
        weight,
        filled: thousandths(number("ratio").unwrap_or(0.0)),
        role: builder.role_prop(node, scopes),
    })
}

/// A tab's label and the value it sets.
fn make_tab<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &mut Scopes<'a>,
    trace: &mut TraceNode,
) -> Option<(String, String)> {
    if node.name != NodeName::Tab {
        trace.absent(Absence::Misplaced);
        return None;
    }
    let label = builder.label(node, scopes);
    let value = literal(node, "value").unwrap_or_else(|| label.clone());
    Some((label, value))
}

/// A select's choices, the one held (or its first) chosen.
fn select<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &mut Scopes<'a>,
    trace: &mut TraceNode,
    id: usize,
) -> NodeKind {
    let declared = builder.expand(&node.children, scopes, &mut trace.children, make_choice);
    let key = builder.key(node);
    let held = builder.env.ui.get(&key);
    let chosen = declared
        .iter()
        .position(|(_, value)| Some(value) == held)
        .unwrap_or(0);
    let focused = builder.focused(&key);
    builder.in_form(node, &key);
    let (options, values): (Vec<String>, Vec<String>) = declared.into_iter().unzip();
    builder.acts.insert(
        id,
        Act::Select {
            key,
            values,
            chosen,
        },
    );
    NodeKind::Select {
        label: builder.label(node, scopes),
        options,
        chosen,
        focused,
    }
}

/// A choice's label and the value it is held as.
fn make_choice<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &mut Scopes<'a>,
    trace: &mut TraceNode,
) -> Option<(String, String)> {
    if node.name != NodeName::Choice {
        trace.absent(Absence::Misplaced);
        return None;
    }
    let label = builder.label(node, scopes);
    let value = literal(node, "value").unwrap_or_else(|| label.clone());
    Some((label, value))
}

/// A line of text as the host holds it, and whether it is typed into.
fn field<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &Scopes<'a>,
    id: usize,
) -> NodeKind {
    let key = builder.key(node);
    let value = builder.env.ui.get(&key).cloned().unwrap_or_default();
    let focused = builder.focused(&key);
    let editing = builder.env.ui.get(EDITING) == Some(&key);
    let form = builder.in_form(node, &key);
    builder.acts.insert(id, Act::Field { key, form });
    NodeKind::Field {
        label: builder.label(node, scopes),
        value: Clean::label(&value).into_string(),
        placeholder: builder
            .text_prop(node, scopes, "placeholder")
            .unwrap_or_default(),
        focused,
        editing,
    }
}

/// The word a form's button says when it names none.
const SUBMIT: &str = "Send";

/// A form: its own button, which sends `fields` to its `action`.
fn form<'a>(
    builder: &mut Builder<'a>,
    node: &'a Compiled,
    scopes: &Scopes<'a>,
    id: usize,
    fields: Vec<(String, String)>,
) -> NodeKind {
    let key = builder.key(node);
    let focused = builder.focused(&key);
    let item = builder
        .prop(node, scopes, "item")
        .map(Cow::into_owned)
        .unwrap_or(Value::Null);
    builder.acts.insert(
        id,
        Act::Form {
            key,
            action: literal(node, "action").unwrap_or_default(),
            item,
            fields,
        },
    );
    NodeKind::Form {
        submit: builder
            .text_prop(node, scopes, "submit")
            .filter(|said| !said.is_empty())
            .unwrap_or_else(|| SUBMIT.to_owned()),
        focused,
    }
}

/// A ratio as thousandths, held to `0..=1000`.
/// A gauge's role: its own until its ratio reaches the `warning`
/// threshold, then `warning`, then `danger` past that one.
fn gauge<'a>(builder: &Builder<'a>, node: &'a Compiled, scopes: &Scopes<'a>) -> NodeKind {
    let number = |name: &str| {
        builder
            .prop(node, scopes, name)
            .and_then(|value| as_number(&value))
    };
    let ratio = number("ratio").unwrap_or(0.0).clamp(0.0, 1.0);
    let reached = |name: &str| number(name).is_some_and(|threshold| ratio >= threshold);
    let role = if reached("danger") {
        Role::Danger
    } else if reached("warning") {
        Role::Warning
    } else {
        builder.role_prop(node, scopes)
    };
    NodeKind::Gauge {
        label: builder.label(node, scopes),
        filled: thousandths(ratio),
        role,
    }
}

/// The largest series a chart draws, so a fixture cannot widen a view
/// without limit.
const CHART_MAX: usize = 512;
/// How tall a bar chart is when it does not say.
const BARS_HEIGHT: u16 = 4;
/// The tallest a bar chart may be.
const BARS_MAX: u16 = 24;

/// A chart's series as thousandths of its largest value: negative and
/// non-numeric values draw as nothing.
/// A chart whose data is not numbers says so: drawing what could not be
/// read as zero would show a fall that never happened.
fn chart_notice(text: &str) -> NodeKind {
    NodeKind::Notice {
        text: Clean::label(text).into_string(),
        role: Role::Warning,
    }
}

fn chart<'a>(builder: &Builder<'a>, node: &'a Compiled, scopes: &Scopes<'a>) -> NodeKind {
    let series = builder.prop(node, scopes, "series");
    let values: Vec<f64> = match series.as_deref() {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => {
            let read: Vec<Option<f64>> = list.iter().take(CHART_MAX).map(as_number).collect();
            let unread = read.iter().filter(|value| value.is_none()).count();
            if unread > 0 {
                return chart_notice(&format!(
                    "Chart: {unread} of {} values are not numbers",
                    read.len()
                ));
            }
            read.into_iter()
                .flatten()
                .map(|value| value.max(0.0))
                .collect()
        }
        Some(_) => return chart_notice("Chart: its series is not a list of numbers"),
    };
    let top = values.iter().copied().fold(0.0_f64, f64::max);
    let levels = values
        .iter()
        .map(|value| match top > 0.0 {
            true => thousandths(value / top),
            false => 0,
        })
        .collect();
    let kind = match builder
        .prop(node, scopes, "kind")
        .as_deref()
        .and_then(Value::as_str)
    {
        Some("bar") => ChartKind::Bar,
        _ => ChartKind::Sparkline,
    };
    let height = match kind {
        ChartKind::Sparkline => 1,
        ChartKind::Bar => match builder.count_prop(node, scopes, "height") {
            0 => BARS_HEIGHT,
            asked => asked.min(BARS_MAX),
        },
    };
    NodeKind::Chart {
        kind,
        levels,
        height,
        role: builder.role_prop(node, scopes),
    }
}

fn thousandths(ratio: f64) -> u16 {
    (ratio * 1000.0).round().clamp(0.0, 1000.0) as u16
}

fn align(value: Option<&Value>) -> Align {
    match value.and_then(Value::as_str) {
        Some("center") => Align::Center,
        Some("end") => Align::End,
        _ => Align::Start,
    }
}

/// The most lines one `Log` keeps: its latest, as a log is read from its
/// end.
const LOG_MAX: usize = 500;

/// A log's lines: a list of them, or one string of several. A line may be
/// an object with `text` and a `role`, for a log whose lines have a tone.
fn log_lines(value: &Value) -> Vec<view::Span> {
    let mut lines = Vec::new();
    let mut push = |text: &str, role: Role| {
        for line in Clean::text(text).lines() {
            lines.push(view::Span::clean(line, role));
        }
    };
    match value {
        Value::Array(entries) => {
            for entry in &entries[entries.len().saturating_sub(LOG_MAX)..] {
                match entry {
                    Value::Object(fields) => push(
                        &fields.get("text").map(scalar_text).unwrap_or_default(),
                        fields.get("role").map(as_role).unwrap_or_default(),
                    ),
                    other => push(&scalar_text(other), Role::Default),
                }
            }
        }
        Value::String(text) => push(text, Role::Default),
        _ => {}
    }
    let keep = lines.len().saturating_sub(LOG_MAX);
    lines.drain(..keep);
    lines
}
