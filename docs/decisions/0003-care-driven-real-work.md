# CodePet's useful-work loop

Status: implemented in v0.4.0; qualification and remaining limits are in [validation](../validation.md).
Recorded: 2026-10-17. Supersedes the earlier interpretation recorded in this file.

The operator rejected removing CodePet's work, visible stats and desktop movement.
After correcting the proposed care direction, they delegated the design:

> figure out the best game loop on your own. i actually don't know how we should
> make it work. i just want a "CodePet"

This is the resulting product choice, not a claim that an exact earlier spec was
recovered. The original implementation spec supplied isolation, review and
acceptance. Its manual task form was subsequently rejected as the main interaction:

> it should be simple. like an icon. it automatically knows where to go find bugs,
> in the user's local repos

That latest correction supersedes the manual-only launch requirement for bounded
read-only scouting. It does not turn a suspected bug into permission to change
the original repository. The intervening mock-only/full-bars gate and observer-only
product direction do not govern CodePet's own work.

## The loop

1. The pet discovers conventional local repository folders and, when ready, scouts
   one isolated read-only source copy for up to five minutes.
2. A suspected bug appears on the pet. Inspect it and explicitly choose **Fix** or
   dismiss it. No finding means no badge and no claim that the repository is bug-free.
3. Fix starts a normal isolated repair. The pet shows progress and offers Cancel.
4. Read the result and the before/after changes. Nothing has touched the original.
5. Explicitly accept/apply. Once only, the pet gains **20 XP** and the matching need
   is filled: bug fixing feeds it, research quenches thirst, building lifts mood.
6. After three accepted jobs or 45 minutes of accepted work, take a two-minute nap.

Food and water suggest matching jobs. Hunger and thirst never require full bars to
work. Free snack/drink controls can revive a fainted pet without a paid task; Play
lifts mood and Bath keeps it clean. These ordinary care actions grant no XP. New
work requires mood at least 25%, a living awake companion, available energy, and
CodePet On. These checks happen in the backend. Failed/cancelled work grants no XP
and incurs no work-energy penalty. During work/review, attended decay is frozen.
No low meter or external coding event authorizes a fix. Automatic scouting is
enabled by default and can be paused. It requires
a started (armed) life, On, awake, happy and available energy. Durable limits allow
one attempt per six hours globally and one per repository per day, including failed
preparation. No automatic retries, fixes or application follow. A pending finding
blocks another scout until Fix or Dismiss. Scouting earns nothing.

The existing folder/task window remains an advanced path for research or custom
coding requests, with explicit Start and provider/network acknowledgement.

## Implementation boundaries

The default build has a separate real-work controller; it does not enable or reuse
the retired mock runner. Codex 0.163.0 is the first adapter. It uses the user's
existing ChatGPT subscription authentication, a fresh provider environment, native
Codex permissions, and no automatic escalation. Other connected coding clients
remain observers. Own jobs load no observer hooks, so they cannot earn a second
reward through the treat jar.

One task owns the slot through review/application. Tasks run in a bounded isolated
copy; the selected original folder is absent from the provider namespace. Cancellation
and deadline tear down the PID namespace, including detached descendants. Application
requires the reviewed digest and explicit acknowledgement of verification limits,
checks current destination bytes, keeps original bytes and an apply journal, and
records its reward in an atomic permanent receipt bound to the original pet.

Keep the existing companion, appearance, name, XP, arming and observer connections.
Keep numeric care/XP displays, dragging and pinning. The task window is a separate
native QML window; closing it does not kill the supervised job or start another.

See [work.md](../work.md) for actual limits, storage and recovery. Historical v0.3.0
archives contain the older observer UI.
