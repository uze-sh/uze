// The `plan` source: the plan's title and its open steps, or nothing when
// the checkout has no plan.
fn answer(input, context) {
    let present = false;
    for entry in list_checkout(".") {
        if entry.name == input.file && entry.kind == "file" { present = true; }
    }
    if !present {
        return #{ data: #{ file: input.file } };
    }
    let title = input.file;
    let open = [];
    for line in lines(read_checkout(input.file)) {
        if line.starts_with("# ") && title == input.file { title = line.sub_string(2); }
        if line.starts_with("- [ ] ") { open.push(#{ text: line.sub_string(6) }); }
    }
    #{ data: #{ file: input.file, title: title, open: open, left: open.len() } }
}
