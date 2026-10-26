// The `live` source's answer. `answer` is asked with the source's `input`
// and returns the envelope: its `data` is what the view draws, in the
// shape of `fixtures/board.json`.
fn answer(input, context) {
    #{
        data: #{
            title: input.title,
            status: "on track",
            notes: "A script answers what a fixture only samples.",
            tasks: [
                #{ name: "Write the script", ratio: 1.0 },
                #{ name: "Enable it in a project", ratio: 0.0 },
            ],
        },
    }
}
