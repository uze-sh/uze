// The `runs` source: the last five runs of the checkout's branch, and the
// repository and branch they were asked for.

// What `.git/<file>` holds, or "" where the checkout has no such file
// (a linked worktree's `.git` is a file pointing outside it).
fn git_file(name) {
    try {
        return read_checkout(`.git/${name}`);
    } catch {
        return "";
    }
}

// `owner/name` of the `origin` remote, when it is on GitHub.
fn origin_repo() {
    let found = regex(git_file("config"), "github\\.com[:/]([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+?)(\\.git)?\\s");
    if found.is_empty() { "" } else { found[0][1] }
}

// A run's state as one word: its conclusion once it is done.
fn state_of(run) {
    if run.status == "completed" && run.conclusion != () { run.conclusion } else { run.status }
}

fn answer(input, context) {
    let repo = input.preferences.repo;
    if repo == "" {
        repo = origin_repo();
    }
    let head = git_file("HEAD");
    let branch = if head.starts_with("ref: refs/heads/") { head.sub_string(16) } else { "" };
    branch.trim();
    if repo == "" || branch == "" {
        return #{
            data: #{ repo: repo, branch: branch, runs: [], last: #{ name: "CI", state: "unknown" } },
            notice: "Name the repository on the Extensions screen",
        };
    }

    let url = `https://api.github.com/repos/${repo}/actions/runs?branch=${branch}&per_page=5`;
    let reply = fetch(url, #{
        secret: "token",
        scheme: "Bearer",
        headers: #{ Accept: "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28" },
    });
    if reply.status != 200 {
        throw `GitHub answered ${reply.status} for ${repo}`;
    }

    let runs = [];
    for run in reply.json.workflow_runs {
        runs.push(#{ name: run.name, number: run.run_number, state: state_of(run) });
    }
    let last = if runs.is_empty() { #{ name: "CI", state: "none" } } else { runs[0] };
    #{ data: #{ repo: repo, branch: branch, runs: runs, last: last } }
}
