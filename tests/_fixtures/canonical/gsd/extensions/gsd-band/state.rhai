// The `state` source: how many lines the milestone's state file holds,
// read from the checkout under `read:checkout`.
fn answer(input, context) {
    let state = read_checkout(".planning/STATE.md");
    #{ data: #{ lines: state.split("\n").len() } }
}
