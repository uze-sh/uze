"""The worklog's MCP server: a month of hours logged against work items.

It speaks MCP over stdio and offers one tool, `worklog_month(month)`, which
the extension and the agents both call. A real tracker would answer from
its own records; this one makes the month up, the same for every call, so
the example works with nothing to connect. A month to come has no entries.
"""

import calendar
import datetime
import json
import random
import sys

TOOL = {
    "name": "worklog_month",
    "description": "Hours logged per day of a month (YYYY-MM), with the work items they went to.",
    "inputSchema": {
        "type": "object",
        "properties": {"month": {"type": "string", "pattern": "^\\d{4}-(0[1-9]|1[0-2])$"}},
        "required": ["month"],
    },
}

ITEMS = [
    ("WL-101", "Search the task list"),
    ("WL-102", "Sync tasks with the calendar"),
    ("WL-103", "Fix the login timeout"),
    ("WL-104", "Write the onboarding guide"),
    ("WL-105", "Review the billing export"),
]
TARGET = 8


def item(identifier, title, hours):
    return {"id": identifier, "title": title, "hours": hours,
            "url": f"https://tracker.example/{identifier}"}


def worklog(month, today):
    year, number = (int(part) for part in month.split("-"))
    chance = random.Random(month)
    last = calendar.monthrange(year, number)[1]
    days = []
    for day in range(1, last + 1):
        date = datetime.date(year, number, day)
        weekend = date.weekday() >= 5
        entries = []
        if not weekend and date <= today and chance.random() > 0.15:
            left = chance.choice([4, 6, 7, 8, 8, 8, 9])
            for identifier, title in chance.sample(ITEMS, chance.randint(1, 3)):
                hours = left if identifier == ITEMS[-1][0] else min(left, chance.choice([2, 3, 4, 5]))
                if hours > 0:
                    entries.append(item(identifier, title, hours))
                    left -= hours
        hours = sum(entry["hours"] for entry in entries)
        days.append({"date": date.isoformat(), "hours": hours, "target": TARGET,
                     "gap": 0 if weekend else max(TARGET - hours, 0),
                     "isWeekend": weekend, "items": entries})
    unlogged = [item(identifier, title, 0) for identifier, title in ITEMS[3:]]
    return {"generatedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "user": "you", "month": month, "targetHours": TARGET,
            "unloggedItems": unlogged, "days": days}


def answer(request):
    method = request.get("method")
    if method == "initialize":
        return {"protocolVersion": request["params"].get("protocolVersion", "2025-06-18"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "worklog", "version": "0.1.0"}}
    if method == "tools/list":
        return {"tools": [TOOL]}
    if method == "tools/call":
        params = request["params"]
        if params.get("name") != TOOL["name"]:
            return {"isError": True, "content": [{"type": "text", "text": f"no tool {params.get('name')}"}]}
        month = params.get("arguments", {}).get("month", "")
        log = worklog(month, datetime.date.today())
        return {"content": [{"type": "text", "text": json.dumps(log)}]}
    return None


def main():
    for line in sys.stdin:
        request = json.loads(line)
        if "id" not in request:
            continue
        result = answer(request)
        reply = {"jsonrpc": "2.0", "id": request["id"]}
        if result is None:
            reply["error"] = {"code": -32601, "message": f"{request.get('method')} is not served"}
        else:
            reply["result"] = result
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
