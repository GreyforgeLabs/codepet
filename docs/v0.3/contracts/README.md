# CodePet v0.3 contracts (WP0)

Frozen inputs for the v0.3 work packages (SPEC-v0.3 §10). Change a contract only together
with its consumers and `check_contracts.py`, in its own commit.

| File | What it fixes | Consumers |
|---|---|---|
| `bar.v2.schema.json` | `bar-state.json` v2 (§8): v1 keys + `enabled`, `agent`, `treats`, `clients`; v0.3.0 compat values | WP1/WP2/WP3 export, WP5 `CodePetBar.js`, WP6 `BarState.js` |
| `fixtures/bar-v2/*.json` | 37 export states (every shared-priority row, routing screens, Helpers row states, the owner's goldfish Ginger before and after Let's go!, robot copy) | QML smokes (WP5/WP6), `priority` / `popup_routing` / `helpers_rows` vectors |
| `fixtures/clients-status/*.json` | `clients.status` results | WP5 smokes, `connect_card` vectors |
| `protocol.json` | kept / removed / added methods, param and result schemas with examples, errors, receipts, Off behaviour of every method, the v0.3 snapshot additions and frozen-reason order, job ids, CLI commands | WP1, WP3, WP5; `codepet-protocol` test |
| `agent_vectors.json` | clients, kinds, states, constants, normalization (incl. the ready-check nonce), event ids, HMAC keys, interactivity, `turn_qualifies`, caps, the session machine, fold outcomes, the `agent::pet_state` aggregate, the shared priority, popup routing, the Connect card, Helpers rows | WP2 (`codepet-core::agent`), WP6 (`petstate.js`), WP5; `codepet-core` test |
| `commands.json`, `shim.sh.template`, `shim-tombstone.sh` | the shim bytes, every stored command and client file, the Codex groups and trust hashes, the ready-check payloads | WP2 (shim, normalizer), WP3 (installers, clients job, doctor) |
| `plugin-payload.schema.json` | what the OpenCode and Hermes plugins send | WP3 (plugins), WP2 (normalizer) |
| `strings.json` | every §3 string, base set `floor` plus `aquatic.` / `robot.` overrides, `help.` and `codex.` adult pages | WP5 (`omarchy-plugin/strings.json`), copy lint |
| `icons.json` | icon sheets and ids, bar badges, `careIconSet` per species, `careWords` | WP4, WP5, WP6 |
| `schema-v5.sql` | v5 DDL and the data steps of the migration | WP1 (migration), WP2 (agent tables) |
| `pixel-budgets.json` | §3 vertical budgets and K3 minimums | WP5 smokes (K3, K13) |
| `notes.md` | offline source reads (Codex async `PermissionRequest`, `grok --help`) and every spec contradiction with its resolution | everyone |
| `tools/gen_*.py` | regenerate `agent_vectors.json`, the fixtures, `commands.json`, the shim files and the payload schema | contract edits |

Check: `python3 docs/v0.3/contracts/check_contracts.py` (standard library only; CI runs it).
It replays every vector through an independent reference model, validates the fixtures
against the schemas, applies the DDL to a stub v4 database, lints the copy (K1, K2, K8,
calm-copy list), recomputes every hash in `commands.json` and runs the shim under `sh` in a
throwaway directory. The reference model is a test oracle, not product code.

Status: **frozen** after the WP0 review round (see `notes.md` §2 items 19-33); WP1 review resolutions in `notes.md` §5; amended after the WP2 review (`notes.md` §6).
