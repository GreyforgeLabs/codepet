.pragma library

// Pure lifecycle rules for the desktop overlay process (SPEC-v0.3 §3.6, §3.8, §7.3, §8). No
// Quickshell, no I/O: shell.qml reads the files and runs the commands. Node-tested by
// qml/tests/test_overlay_flow.mjs.
//
// - The overlay process exits when CodePet is Off (bar-state.json `enabled: false`) or
//   when "Pet on my desktop" is off. That switch is the bar plugin's `overlayOnLoad`
//   setting in ~/.config/omarchy/shell.json (written by the bar plugin; read-only here).
//   Turned off after this process saw it on, it exits at once; read as off before that,
//   it exits only after the off has held for SETTLE_MS (a launch can race the write that
//   turns the switch on).
// - Calm mode is the same entry's `calm` setting, read live (SPEC-v0.3 §3.6, §7.2).
// - A click on the pet opens the bar popup at Home through the Omarchy shell's IPC:
//   `omarchy-shell -q shell summon greyforge.codepet`, which calls the bar widget's
//   `open()` (Omarchy's bar-widget panel contract: open(), close(), opened; the bar plugin
//   makes open() show Home). `-q` keeps it silent and successful when the shell or the
//   widget is not there. Requests closer than HOME_COOLDOWN_MS are coalesced.

var PLUGIN_ID = "greyforge.codepet"
var SETTING = "overlayOnLoad"
var MAX_SETTINGS_BYTES = 1048576
// One Omarchy IPC call per second at most, however fast the pet is clicked.
var HOME_COOLDOWN_MS = 1000
// How long a "Pet on my desktop" off must hold before a process that never saw it on exits.
var SETTLE_MS = 1500
// The packaged Omarchy root, used when $OMARCHY_PATH is not set (omarchy-shell needs it).
var DEFAULT_OMARCHY_PATH = "/usr/share/omarchy"

function isObject(v) {
    return v !== null && typeof v === "object" && !Array.isArray(v)
}

function utf8Bytes(s) {
    var n = 0
    for (var i = 0; i < s.length; i++) {
        var c = s.charCodeAt(i)
        if (c < 0x80) n += 1
        else if (c < 0x800) n += 2
        else if (c >= 0xd800 && c <= 0xdbff) { n += 4; i++ }
        else n += 3
    }
    return n
}

function absolute(v) {
    return typeof v === "string" && v.length > 1 && v.charAt(0) === "/" && v.indexOf("\n") < 0
           && v.indexOf("\0") < 0 && v.split("/").indexOf("..") < 0 ? v.replace(/\/+$/, "") : ""
}

// `$HOME/.config/omarchy/shell.json`, exactly the file the Omarchy shell reads and writes
// (its `userConfigPath` ignores XDG_CONFIG_HOME); "" without an absolute HOME (the watcher
// stays idle).
function settingsPath(home) {
    var h = absolute(home)
    return h === "" ? "" : h + "/.config/omarchy/shell.json"
}

// A parsed shell.json the Omarchy shell would use (a JSON object with `version: 1`), or null
// when the text is missing, oversized, unparsable or of another version.
function parseSettings(text) {
    if (typeof text !== "string" || text.length === 0 || text.length > MAX_SETTINGS_BYTES
            || utf8Bytes(text) > MAX_SETTINGS_BYTES)
        return null
    var doc
    try {
        doc = JSON.parse(text)
    } catch (e) {
        return null
    }
    return isObject(doc) && doc.version === 1 ? doc : null
}

// The CodePet bar entry of a parsed shell.json, or null when the bar has no CodePet cell.
function entryOf(doc) {
    if (!isObject(doc) || !isObject(doc.bar))
        return null
    var holders = [doc.bar.layout, doc.bar]
    var sections = ["left", "center", "right"]
    for (var h = 0; h < holders.length; h++) {
        if (!isObject(holders[h]))
            continue
        for (var s = 0; s < sections.length; s++) {
            var list = holders[h][sections[s]]
            if (!Array.isArray(list))
                continue
            for (var i = 0; i < list.length; i++) {
                if (isObject(list[i]) && list[i].id === PLUGIN_ID)
                    return list[i]
            }
        }
    }
    return null
}

// The CodePet bar entry's settings in a shell.json text, or null (no usable file, or no
// CodePet cell).
function pluginEntry(text) {
    return entryOf(parseSettings(text))
}

// "Pet on my desktop": true / false from the CodePet bar entry. A missing key is the
// manifest default, false, exactly as the bar plugin reads it. A shell.json the Omarchy
// shell uses but without a CodePet cell is false too: the cell was removed from the bar.
// null (unknown, keep running) only without a usable file: none, unparsable, oversized or
// not `version: 1` (the Omarchy shell then ignores it as well).
function desktopPetSetting(text) {
    var doc = parseSettings(text)
    if (doc === null)
        return null
    var entry = entryOf(doc)
    return entry !== null && String(entry[SETTING]) === "true"
}

// Calm mode: the CodePet bar entry's `calm` setting (missing = the manifest default, off);
// null when there is no usable file or no CodePet cell.
function calmSetting(text) {
    var entry = pluginEntry(text)
    return entry === null ? null : String(entry.calm) === "true"
}

// Placement is a preference, separate from care/progression. Invalid coordinates
// fall back to the floor; an unknown monitor falls back to the connected first screen.
function placementSetting(text) {
    var entry = pluginEntry(text)
    var p = entry && isObject(entry.desktopPlacement) ? entry.desktopPlacement : {}
    var valid = typeof p.x === "number" && isFinite(p.x) && p.x >= 0 && p.x <= 1
             && typeof p.y === "number" && isFinite(p.y) && p.y >= 0 && p.y <= 1
    return { x: valid ? p.x : null, y: valid ? p.y : null, pinned: p.pinned === true,
             screen: typeof p.screen === "string" && p.screen.length <= 256 ? p.screen : "" }
}

// One explicit preference write, via the host's config mutator, never a shell string.
function placementArgv(omarchyPath, position) {
    var base = absolute(omarchyPath) || DEFAULT_OMARCHY_PATH
    return ["qs", "ipc", "-p", base + "/shell", "call", "--", "shell", "setBarWidget",
            PLUGIN_ID, "desktopPlacement", JSON.stringify(position), "{}"]
}

// Whether the pet moves calmly: an explicit `setReducedMotion` call (`override`, null when
// none) wins until the setting changes; then the bar entry's `calm` (`setting`); with no
// setting, CODEPET_REDUCED_MOTION=1 from the launch (`envValue`). Omarchy has no system
// reduced-motion signal, so "follows system" is the plugin default: off.
function reducedMotion(override, setting, envValue) {
    if (override === true || override === false)
        return override
    if (setting === true || setting === false)
        return setting
    return envValue === "1"
}

// Why the overlay process must exit now: "off", "desktop_pet_off" or "" (keep running).
// `barState` is the last valid bar-state.json (null before the first read).
function exitReason(barState, desktopPet) {
    if (isObject(barState) && barState.enabled === false)
        return "off"
    if (desktopPet === false)
        return "desktop_pet_off"
    return ""
}

// Whether an exit reason ends the process now: Off always does; a desktop pet switched
// off does once this process saw it on. Otherwise the off must hold for SETTLE_MS first.
function exitsAtOnce(reason, sawDesktopPetOn) {
    return reason === "off" || (reason === "desktop_pet_off" && sawDesktopPetOn === true)
}

// Whether a Home request goes out now: none was sent in the last HOME_COOLDOWN_MS
// (`lastSentMs` 0 = never; a clock that went backwards allows one).
function homeDue(lastSentMs, nowMs) {
    if (!(typeof lastSentMs === "number" && lastSentMs > 0))
        return true
    return nowMs < lastSentMs || nowMs - lastSentMs >= HOME_COOLDOWN_MS
}

// argv that opens the bar popup at Home. `omarchyPath` is $OMARCHY_PATH (absolute); without
// it, omarchy-shell from PATH is run with the packaged OMARCHY_PATH, because it refuses to
// run (silently, under -q) when OMARCHY_PATH is not set.
function openHomeArgv(omarchyPath) {
    var base = absolute(omarchyPath)
    var call = ["-q", "shell", "summon", PLUGIN_ID]
    if (base !== "")
        return [base + "/bin/omarchy-shell"].concat(call)
    return ["/usr/bin/env", "OMARCHY_PATH=" + DEFAULT_OMARCHY_PATH, "omarchy-shell"].concat(call)
}

function openUpdatesArgv(omarchyPath) {
    var base = absolute(omarchyPath)
    var call = ["-q", PLUGIN_ID, "openUpdates"]
    if (base !== "")
        return [base + "/bin/omarchy-shell"].concat(call)
    return ["/usr/bin/env", "OMARCHY_PATH=" + DEFAULT_OMARCHY_PATH, "omarchy-shell"].concat(call)
}

// The suggested show/hide hotkey (README "Desktop pet"): bind it to `codepet-overlay toggle`.
var TOGGLE_KEY_HINT = "Super + Alt + P"

// The one toast after the pet's HIDE pill: how to bring the pet back. A plain argv for
// Quickshell.execDetached; notify-send never sees a shell.
function hiddenToastArgv() {
    return ["notify-send", "--app-name=CodePet", "--expire-time=5000",
            "Your pet is hiding", "Press " + TOGGLE_KEY_HINT + " to bring it back."]
}
