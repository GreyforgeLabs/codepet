# CodePet v0.3 contracts (WP0)

Frozen inputs for the v0.3 work packages (SPEC-v0.3 §10). Change a contract only together
with its consumers and `check_contracts.py`, in its own commit.

| File | What it fixes | Consumers |
|---|---|---|
| `bar.v2.schema.json` | `bar-state.json` v2 (§8): v1 keys + `enabled`, `agent`, `treats`, `clients`; v0.3.0 compat values | WP1/WP2/WP3 export, WP5 `CodePetBar.js`, WP6 `BarState.js` |
| `fixtures/bar-v2/*.json` | 26 export states (every shared-priority row, the owner's goldfish Ginger, robot copy, mixed client rows) | QML smokes (WP5/WP6), `priority` vectors |
| `protocol.json` | kept / removed / added methods, param schemas with valid and invalid examples, new error codes, CLI commands | WP1, WP3; `codepet-protocol` test |
| `agent_vectors.json` | clients, kinds, states, constants, normalization, event ids, interactivity, `turn_qualifies`, caps, the session machine, fold outcomes, the shared priority | WP2 (`codepet-core::agent`), WP6 (`petstate.js`); `codepet-core` test |
| `strings.json` | every §3 string, base set `floor` plus `aquatic.` / `robot.` overrides, `help.` and `codex.` adult pages | WP5 (`omarchy-plugin/strings.json`), copy lint |
| `icons.json` | icon sheets and ids, bar badges, `careIconSet` per species, `careWords` | WP4, WP5, WP6 |
| `schema-v5.sql` | v5 DDL and the data steps of the migration | WP1 (migration), WP2 (agent tables) |
| `pixel-budgets.json` | §3 vertical budgets and K3 minimums | WP5 smokes (K3, K13) |
| `notes.md` | offline source reads (Codex async `PermissionRequest`, `grok --help`) and every spec contradiction with its resolution | everyone |
| `tools/gen_*.py` | regenerate `agent_vectors.json` and the fixtures | contract edits |

Check: `python3 docs/v0.3/contracts/check_contracts.py` (standard library only; CI runs it).
It replays every vector through an independent reference model, validates the fixtures
against the schema, applies the DDL to a stub v4 database and lints the copy (K1, K2, K8,
calm-copy list). The reference model is a test oracle, not product code.
