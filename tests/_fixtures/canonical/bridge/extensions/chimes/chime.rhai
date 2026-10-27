// The `chime` source, asked once per event: the cue it sounds and what
// the status row says. Before any event it is quiet.
fn answer(input, context) {
    if !("event" in input) {
        return #{ data: #{} };
    }
    let event = input.event;
    if event.name == "agent.approval.asked" {
        #{
            data: #{ said: "asks approval", agent: event.agent },
            notify: #{ title: "Approval", body: `An agent on ${event.harness} asks for an approval` },
            sound: "attention",
        }
    } else {
        #{
            data: #{ said: "turn finished", agent: event.agent },
            sound: "done",
        }
    }
}
