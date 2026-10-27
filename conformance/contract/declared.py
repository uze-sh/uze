"""Asking every harness every question, and letting it answer "no".

A binding may decline part of a contract (`bindings.unsupported`), and the
answer is a result: it sits in the evidence beside the passes, with its
reason, and review can disagree with it. What it may not be is a constant.
A declaration used to be `check(name, True, reason)`, which read the same
whether or not the harness still lacked the control — so a vendor that
shipped it, or a fixture that stopped exercising it, changed nothing.

Here the measurement always runs. A declined claim records what the run
saw as `declare(holds=...)`; an undeclined one records it as the ordinary
check it is. The gate then decides what a declaration's measurement means
on this version (`gate.py`).
"""

from shared.common import check, check_absence, declare

#: Each declined presence this run measured, by its claim: True when the
#: capability was missing, as the declaration says.
_missing = {}


def presence(bindings, name, prop, observed, detail, measured=True):
    """A capability that must be there: `observed` is whether it was.

    Declined, the limitation holds when it was *not* observed. `measured`
    is False when the run never reached the point where it could look,
    which a declaration records as unproven rather than as either answer.
    """
    reason = bindings.unsupported(prop)
    if reason is None:
        check(name, measured and observed, detail)
        return
    _missing[prop] = measured and not observed
    declare(
        name,
        (not observed) if measured else None,
        f"{bindings.harness} cannot: {reason}",
        detail,
    )


def absence(bindings, name, prop, violated, *, settled, proof, detail):
    """A behaviour that must not happen: `violated` is whether it did.

    The absence is only evidence once the turn settled and `proof` showed
    the surface was populated (`check_absence`). Declined, the limitation
    holds when the behaviour happened, under the same two conditions.
    Declined through a presence this run measured missing, the surface can
    never be populated: it holds on a settled turn, and escalates with that
    presence the day the capability arrives.
    """
    reason = bindings.unsupported(prop)
    if reason is None:
        check_absence(name, not violated, settled, proof=proof, detail=detail)
        return
    if not proof and _missing.get(prop):
        declare(
            name,
            True if settled else None,
            f"{bindings.harness} cannot: {reason}",
            f"{detail} (unobservable while `{prop}` is missing)",
        )
        return
    declare(
        name,
        violated if (settled and proof) else None,
        f"{bindings.harness} cannot: {reason}",
        detail,
    )
