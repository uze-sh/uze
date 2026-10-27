// The `kennel` source: the pet's name, kept in the store, and its mood,
// content for ten minutes after it was fed and hungry after.
fn answer(input, context) {
    let content_for = 600000;
    let kept = store();
    let name = kept.name ?? "Pixel";
    let fed = kept.fed ?? 0;
    let content = fed > 0 && clock() - (kept.last_fed ?? 0) < content_for;
    #{
        data: #{
            name: name,
            fed: fed,
            mood: if content { "content" } else { "hungry" },
            said: if content { `content, fed ${fed} times` } else { "hungry" },
        },
        store: #{ name: name },
    }
}
