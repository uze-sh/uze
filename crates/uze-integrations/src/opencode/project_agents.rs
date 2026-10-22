//! A project's own `.agents/agents` on OpenCode: a section of UZE's one
//! plugin that registers them into the registry of the location it serves.
//!
//! OpenCode reads no `.agents/agents`, and no launch can hand its shared
//! background service one project's agents from outside the checkout
//! (`experiments/opencode/project-agents`). A plugin can: it is set up once
//! per location, handed that location, and may register agents into its
//! registry and reload it (2.1.2, `experiments/opencode/project-agents-plugin`).
//!
//! The section reads the agents the way `shared::agent::project_agents`
//! does and projects each with the dialect `agent_block` applies, handed
//! over as data from [`OPENCODE_AGENT_DIALECT`](super::OPENCODE_AGENT_DIALECT)
//! so the two cannot keep different fields. It writes the resolved agent,
//! not configuration, so it converts the way OpenCode's own configured-agent
//! plugin does. Precedence is OpenCode's: external plugins run after its
//! built-in agents and before its configured ones
//! (`experiments/opencode/project-agents-precedence`), so a project agent
//! with a built-in's id is skipped, and a configured agent of the same id
//! applies over the project's.

use uze_core::capability::harness::IDENTITY_FIELDS;

use super::runtime_plugin::Section;
use crate::shared::agent::MAX_PROJECT_AGENT_BYTES;
use crate::shared::dialect::{AgentDialect, to_json};

pub(super) fn section(dialect: &AgentDialect, keys: &[&str]) -> Section {
    let json = |value: serde_json::Value| value.to_string();
    let dialect = json(to_json(dialect));
    let keys = json(serde_json::json!(keys));
    let identity = json(serde_json::json!(IDENTITY_FIELDS));
    Section {
        name: "project-agents",
        source: format!(
            r#"async (ctx) => {{
    const DIALECT = {dialect};
    const KEYS = {keys};
    const IDENTITY = {identity};
    const MAX_BYTES = {MAX_PROJECT_AGENT_BYTES};
    // The actions OpenCode resolves as paths: only their `~` is expanded.
    const PATH_ACTIONS = ["external_directory", "read", "edit"];
    // `directory` is the checkout's own top level, a linked one's included;
    // `canonical` would be the main checkout's.
    const checkout = ctx.location.project?.directory ?? ctx.location.directory;
    const root = join(checkout, ".agents", "agents");
    const isMap = (value) => typeof value === "object" && value !== null && !Array.isArray(value);
    const byComponents = (a, b) => {{
      const left = a.split("/");
      const right = b.split("/");
      for (let i = 0; i < Math.min(left.length, right.length); i++) {{
        if (left[i] !== right[i]) return left[i] < right[i] ? -1 : 1;
      }}
      return left.length - right.length;
    }};

    // Every `*.md` at any depth, never through a link, in path order.
    const directories = () => {{
      const found = [];
      const pending = [root];
      while (pending.length) {{
        const dir = pending.pop();
        let stat;
        try {{
          stat = lstatSync(dir);
        }} catch {{
          continue;
        }}
        if (!stat.isDirectory()) continue;
        found.push(dir);
        let names = [];
        try {{
          names = readdirSync(dir);
        }} catch {{}}
        for (const name of names) pending.push(join(dir, name));
      }}
      return found;
    }};
    const files = () => {{
      const found = [];
      for (const dir of directories()) {{
        let names = [];
        try {{
          names = readdirSync(dir);
        }} catch {{}}
        for (const name of names) {{
          if (name === ".md" || !name.endsWith(".md")) continue;
          const path = join(dir, name);
          try {{
            const stat = lstatSync(path);
            if (stat.isFile() && stat.size <= MAX_BYTES) found.push(path);
          }} catch {{}}
        }}
      }}
      return found.sort(byComponents);
    }};

    const split = (text) => {{
      const rest = text.startsWith("---\n") ? text.slice(4) : text.startsWith("---\r\n") ? text.slice(5) : null;
      if (rest === null) return null;
      let offset = 0;
      for (const line of rest.split(/(?<=\n)/)) {{
        if (offset > 0 && line.endsWith("\n") && line.replace(/[\r\n]+$/, "") === "---") {{
          const head = rest.slice(0, offset - 1);
          return [head.endsWith("\r") ? head.slice(0, -1) : head, rest.slice(offset + line.length)];
        }}
        offset += line.length;
      }}
      return null;
    }};
    const parse = (text) => {{
      const parts = split(text);
      if (!parts) return {{ frontmatter: {{}}, body: text }};
      let parsed;
      try {{
        parsed = Bun.YAML.parse(parts[0]);
      }} catch {{
        return null;
      }}
      if (parsed === null || parsed === undefined) parsed = {{}};
      if (!isMap(parsed)) return null;
      return {{ frontmatter: parsed, body: parts[1] }};
    }};
    const text = (value) => (typeof value === "string" && value.trim() ? value.trim() : undefined);

    const block = (frontmatter) => {{
      const blocks = frontmatter.harness;
      if (blocks === undefined || blocks === null) return {{}};
      if (!isMap(blocks)) return {{}};
      if (Object.values(blocks).some((value) => value !== null && !isMap(value))) return {{}};
      for (const key of KEYS) if (isMap(blocks[key])) return blocks[key];
      return {{}};
    }};
    const conforms = (known, value) => {{
      switch (known.shape) {{
        case "text": return typeof value === "string" && value.trim() !== "";
        case "text-or-list": return typeof value === "string" || (Array.isArray(value) && value.every((item) => typeof item === "string"));
        case "one-of":
        case "ceiling": return typeof value === "string" && known.choices.includes(value);
        case "qualified": {{
          if (typeof value !== "string") return false;
          const at = value.indexOf("/");
          return at > 0 && at < value.length - 1;
        }}
        case "flag": return typeof value === "boolean";
        case "count": return Number.isInteger(value) && value > 0;
        case "hex-color": return typeof value === "string" && /^#[0-9a-fA-F]{{6}}$/.test(value);
        case "rules": return Array.isArray(value) && value.every((rule) => isMap(rule)
          && typeof rule.action === "string" && typeof rule.resource === "string"
          && ["allow", "deny", "ask"].includes(rule.effect));
        default: return false;
      }}
    }};
    // A conforming value lowered to what an authored agent may hold, or
    // `undefined` when nothing of it is left to carry.
    const narrowed = (known, value) => {{
      if (known.shape === "rules") {{
        const narrowing = value.filter((rule) => rule.effect !== "allow");
        return narrowing.length ? narrowing : undefined;
      }}
      if (known.shape === "ceiling" && known.choices.indexOf(value) > known.choices.indexOf(known.ceiling)) {{
        return known.ceiling;
      }}
      return value;
    }};
    const carried = (frontmatter) => {{
      const fields = {{}};
      for (const [field, value] of Object.entries(block(frontmatter))) {{
        if (IDENTITY.includes(field)) continue;
        const known = DIALECT.known[field];
        if (!known) {{
          if (DIALECT.carriesUnknown) fields[field] = value;
          continue;
        }}
        if (known.shape === "left-out" || !conforms(known, value)) continue;
        const kept = narrowed(known, value);
        if (kept !== undefined) fields[field] = kept;
      }}
      return fields;
    }};

    const agents = () => {{
      const seen = new Set();
      const found = [];
      for (const path of files()) {{
        let document;
        try {{
          document = parse(readFileSync(path, "utf8"));
        }} catch {{
          continue;
        }}
        if (!document) continue;
        const id = text(document.frontmatter.name) ?? basename(path).slice(0, -3);
        if (seen.has(id)) continue;
        seen.add(id);
        found.push({{ id, document }});
      }}
      return found;
    }};
    const expandHome = (resource, home) => {{
      if (resource === "~" || resource === "$HOME") return home;
      if (resource.startsWith("~/")) return join(home, resource.slice(2));
      if (resource.startsWith("$HOME/")) return join(home, resource.slice(6));
      return resource;
    }};
    const apply = (item, id, document, fields) => {{
      item.name = id;
      const description = text(document.frontmatter.description);
      if (description !== undefined) item.description = description;
      item.system = document.body.trim();
      item.mode = fields.mode ?? "subagent";
      if (fields.model !== undefined) {{
        const at = fields.model.indexOf("/");
        item.model = {{ providerID: fields.model.slice(0, at), id: fields.model.slice(at + 1) }};
      }}
      if (fields.color !== undefined) item.color = fields.color;
      if (fields.steps !== undefined) item.steps = fields.steps;
      if (fields.hidden !== undefined) item.hidden = fields.hidden;
      if (fields.permissions !== undefined) {{
        const home = homedir();
        item.permissions.push(...fields.permissions.map((rule) => ({{
          action: rule.action,
          resource: PATH_ACTIONS.includes(rule.action) ? expandHome(rule.resource, home) : rule.resource,
          effect: rule.effect,
        }})));
      }}
    }};

    const registered = new Set();
    await ctx.agent.transform((editor) => {{
      for (const {{ id, document }} of agents()) {{
        if (!registered.has(id) && editor.get(id)) continue;
        const fields = carried(document.frontmatter);
        if (fields.disabled === true) continue;
        registered.add(id);
        editor.update(id, (item) => apply(item, id, document, fields));
      }}
    }});

    // One watch per directory, kept in step with the tree: a recursive
    // watch on Linux reports only its own directory's entries.
    const watchers = new Map();
    const reload = () => {{
      sync();
      ctx.agent.reload().catch((error) => console.error(`uze project-agents: ${{error}}`));
    }};
    const sync = () => {{
      const wanted = new Set([checkout, join(checkout, ".agents"), ...directories()]);
      for (const [dir, watcher] of watchers) {{
        if (!wanted.has(dir)) {{
          watcher.close();
          watchers.delete(dir);
        }}
      }}
      for (const dir of wanted) {{
        if (watchers.has(dir)) continue;
        try {{
          const shallow = dir === checkout || dir === join(checkout, ".agents");
          watchers.set(dir, watch(dir, (event, name) => {{
            if (!shallow || String(name).startsWith(dir === checkout ? ".agents" : "agents")) reload();
          }}));
        }} catch {{}}
      }}
    }};
    sync();
    return () => {{
      for (const watcher of watchers.values()) watcher.close();
    }};
  }}"#
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use serde_json::{Value, json};
    use uze_core::capability::harness::yaml as serde_yaml;

    use super::*;
    use crate::opencode::OPENCODE_AGENT_DIALECT;
    use crate::shared::{agent::project_agents, dialect::agent_block};

    const KEYS: &[&str] = &["opencode"];
    const HOME: &str = "/home/lab";

    /// Every rule the two readings must agree on, one file each.
    fn fixtures(agents: &Path) {
        let write = |name: &str, content: &str| {
            let path = agents.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        };
        write(
            "full.md",
            "---\nname: reviewer\ndescription: Reviews it\nmodel: haiku\ntools: Read\nharness:\n  opencode:\n    model: anthropic/claude-haiku\n    color: \"#a0b1c2\"\n    mode: all\n    steps: 3\n    hidden: true\n    temperature: 0.2\n    unknown: x\n    description: not here\n    permissions:\n      - { action: shell, resource: \"*\", effect: allow }\n      - { action: read, resource: \"~/secrets/*\", effect: deny }\n      - { action: shell, resource: \"rm *\", effect: ask }\n---\n\nReview it.\n\n",
        );
        write("plain.md", "No frontmatter at all.\n");
        write("unclosed.md", "---\ndescription: never closed\n");
        write("broken.md", "---\ndescription: [unbalanced\n---\nbody\n");
        write("list.md", "---\n- a\n- b\n---\nbody\n");
        write(
            "other-block.md",
            "---\ndescription: d\nharness:\n  claude: not-a-map\n  opencode: { model: a/b }\n---\nbody\n",
        );
        write(
            "disabled.md",
            "---\nharness: { opencode: { disabled: true } }\n---\nbody\n",
        );
        write(
            "wrong-shapes.md",
            "---\nharness: { opencode: { model: haiku, color: red, steps: 0, mode: primary, hidden: yes } }\n---\nbody\n",
        );
        write(
            "only-allow.md",
            "---\nharness: { opencode: { permissions: [{ action: edit, resource: \"*\", effect: allow }] } }\n---\nbody\n",
        );
        write("a/dup.md", "---\ndescription: first by path\n---\nfirst\n");
        write(
            "b-dup.md",
            "---\nname: dup\ndescription: second by path\n---\nsecond\n",
        );
        write(
            "nested/deep/inner.md",
            "---\ndescription: deep\n---\ninner\n",
        );
        write(
            "build.md",
            "---\ndescription: the project's build\n---\nbuild\n",
        );
        write("notes.txt", "not an agent");
        write(".md", "---\ndescription: no stem\n---\nx\n");
        fs::write(
            agents.join("huge.md"),
            vec![b'x'; MAX_PROJECT_AGENT_BYTES as usize + 1],
        )
        .unwrap();
        std::os::unix::fs::symlink(agents.join("plain.md"), agents.join("linked.md")).unwrap();
    }

    /// What Rust's reading resolves each agent to, in the shape the
    /// section writes: `disabled` and a built-in's id leave it out.
    fn expected(agents: &Path, builtins: &[&str]) -> Value {
        let mut out = serde_json::Map::new();
        for agent in project_agents(agents) {
            if builtins.contains(&agent.label.as_str()) {
                continue;
            }
            let (carried, _) = agent_block(&OPENCODE_AGENT_DIALECT, KEYS, &agent.document);
            let field = |name: &str| carried.get(name);
            if field("disabled").and_then(serde_yaml::Value::as_bool) == Some(true) {
                continue;
            }
            let mut item = json!({
                "name": agent.label,
                "system": agent.document.body.trim(),
                "mode": field("mode").and_then(serde_yaml::Value::as_str).unwrap_or("subagent"),
                "permissions": [],
            });
            if let Some(description) = &agent.document.description {
                item["description"] = json!(description);
            }
            if let Some((provider, model)) = field("model")
                .and_then(serde_yaml::Value::as_str)
                .and_then(|model| model.split_once('/'))
            {
                item["model"] = json!({ "providerID": provider, "id": model });
            }
            if let Some(color) = field("color").and_then(serde_yaml::Value::as_str) {
                item["color"] = json!(color);
            }
            if let Some(steps) = field("steps").and_then(serde_yaml::Value::as_u64) {
                item["steps"] = json!(steps);
            }
            if let Some(hidden) = field("hidden").and_then(serde_yaml::Value::as_bool) {
                item["hidden"] = json!(hidden);
            }
            if let Some(rules) = field("permissions").and_then(serde_yaml::Value::as_sequence) {
                item["permissions"] = rules
                    .iter()
                    .map(|rule| {
                        let text = |name: &str| rule.get(name).and_then(serde_yaml::Value::as_str).unwrap();
                        let resource = text("resource");
                        let resource = match (text("action"), resource.strip_prefix("~/")) {
                            ("read" | "edit" | "external_directory", Some(rest)) => format!("{HOME}/{rest}"),
                            _ => resource.to_owned(),
                        };
                        json!({ "action": text("action"), "resource": resource, "effect": text("effect") })
                    })
                    .collect();
            }
            out.insert(agent.label, item);
        }
        Value::Object(out)
    }

    /// The section, set up against `checkout` by a context that records the
    /// registry it builds over one built-in agent.
    fn registered(bun: &str, root: &Path, checkout: &Path) -> Value {
        let plugin =
            crate::opencode::runtime_plugin::source(&[section(&OPENCODE_AGENT_DIALECT, KEYS)]);
        fs::write(root.join("uze.ts"), plugin).unwrap();
        let checkout = serde_json::to_string(&checkout.to_string_lossy()).unwrap();
        fs::write(
            root.join("drive.ts"),
            format!(
                r#"import plugin from "./uze.ts";
const transforms = [];
const errors = [];
console.error = (...parts) => errors.push(parts.join(" "));
const cleanup = await plugin.setup({{
  location: {{ directory: {checkout} + "/sub", project: {{ directory: {checkout} }} }},
  agent: {{ transform: async (fn) => transforms.push(fn), reload: async () => {{}} }},
}});
const registry = new Map([["build", {{ name: "Build", permissions: [] }}]]);
const editor = {{
  list: () => [...registry.values()],
  get: (id) => registry.get(id),
  default: () => {{}},
  update: (id, fn) => {{
    const item = registry.get(id) ?? {{ permissions: [] }};
    registry.set(id, item);
    fn(item);
  }},
  remove: (id) => registry.delete(id),
}};
for (const fn of transforms) fn(editor);
cleanup();
registry.delete("build");
console.log(JSON.stringify({{ agents: Object.fromEntries(registry), errors }}));
"#
            ),
        )
        .unwrap();
        let output = Command::new(bun)
            .args(["run", "drive.ts"])
            .current_dir(root)
            .env("HOME", HOME)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reported: Value = serde_json::from_slice(output.stdout.trim_ascii()).unwrap();
        assert_eq!(reported["errors"], json!([]), "{reported}");
        reported["agents"].clone()
    }

    #[test]
    fn the_section_registers_exactly_what_the_rust_reading_projects() {
        let Some(bun) = crate::opencode::bun_for_tests() else {
            return;
        };
        let root = uze_testkit::temp::scratch("opencode-project-agents-parity");
        let checkout = root.join("checkout");
        let agents = checkout.join(".agents").join("agents");
        fs::create_dir_all(&agents).unwrap();
        fixtures(&agents);

        let want = expected(&agents, &["build"]);
        assert_eq!(
            want["reviewer"]["permissions"],
            json!([
                { "action": "read", "resource": "/home/lab/secrets/*", "effect": "deny" },
                { "action": "shell", "resource": "rm *", "effect": "ask" },
            ]),
            "the fixture exercises narrowing and `~`: {want}"
        );
        assert_eq!(
            want.as_object().unwrap().keys().collect::<Vec<_>>(),
            [
                "dup",
                "reviewer",
                "inner",
                "only-allow",
                "other-block",
                "plain",
                "unclosed",
                "wrong-shapes"
            ],
            "path order, first label wins, and every file the rules leave out is out: {want}"
        );
        assert_eq!(want["dup"]["system"], "first", "{want}");
        assert_eq!(
            want["other-block"].get("model"),
            None,
            "a block that is not a mapping empties every block: {want}"
        );
        assert_eq!(
            want["wrong-shapes"],
            json!({ "name": "wrong-shapes", "system": "body", "mode": "subagent", "permissions": [] }),
            "{want}"
        );
        assert_eq!(registered(bun, &root, &checkout), want);
        let _ = fs::remove_dir_all(root);
    }
}
