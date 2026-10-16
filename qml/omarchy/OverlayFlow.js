.pragma library

// Pure lifecycle rules for the desktop overlay process (SPEC-v0.3 §3.8, §7.3, §8). No
// Quickshell, no I/O: shell.qml reads the files and runs the commands. Node-tested by
// qml/tests/test_overlay_flow.mjs.
//
// - The overlay process exits when CodePet is Off (bar-state.json `enabled: false`) or
//   when "Pet on my desktop" is off. That switch is the bar plugin's `overlayOnLoad`
//   setting in ~/.config/omarchy/shell.json (written by the bar plugin; read-only here).
// - A click on the pet opens the bar popup at Home through the Omarchy shell's IPC:
//   `omarchy-shell -q shell summon greyforge.codepet`, which calls the bar widget's
//   `open()` (Omarchy's bar-widget panel contract: open(), close(), opened; the bar plugin
//   makes open() show Home). `-q` keeps it silent and successful when the shell or the
//   widget is not there.

var PLUGIN_ID = "greyforge.codepet"
var SETTING = "overlayOnLoad"
var MAX_SETTINGS_BYTES = 1048576
// Verbs that open the bar popup. `panel` is the v0.3 name; the v0.2 window verbs stay as
// aliases while the v0.2 bar menu and launcher still send them (until WP5 replaces them).
var HOME_VERBS = ["panel", "tasks", "care", "companion", "life"]

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

// `$XDG_CONFIG_HOME/omarchy/shell.json` (a relative XDG_CONFIG_HOME is ignored), else
// `~/.config/omarchy/shell.json`; "" without an absolute base (the watcher stays idle).
function settingsPath(xdgConfigHome, home) {
    var base = absolute(xdgConfigHome)
    if (base === "") {
        var h = absolute(home)
        base = h === "" ? "" : h + "/.config"
    }
    return base === "" ? "" : base + "/omarchy/shell.json"
}

// The CodePet bar entry's settings in a shell.json text, or null when the file is
// missing, oversized or unparsable, or has no CodePet entry.
function pluginEntry(text) {
    if (typeof text !== "string" || text.length === 0 || text.length > MAX_SETTINGS_BYTES
            || utf8Bytes(text) > MAX_SETTINGS_BYTES)
        return null
    var doc
    try {
        doc = JSON.parse(text)
    } catch (e) {
        return null
    }
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

// "Pet on my desktop": true / false from the CodePet bar entry, null when unknown (no
// shell.json, no CodePet cell: a standalone overlay keeps running). A missing key is the
// manifest default, false, exactly as the bar plugin reads it.
function desktopPetSetting(text) {
    var entry = pluginEntry(text)
    if (entry === null)
        return null
    return String(entry[SETTING]) === "true"
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

// argv that opens the bar popup at Home. `omarchyPath` is $OMARCHY_PATH (absolute), else
// `omarchy-shell` is looked up on PATH.
function openHomeArgv(omarchyPath) {
    var base = absolute(omarchyPath)
    var bin = base === "" ? "omarchy-shell" : base + "/bin/omarchy-shell"
    return [bin, "-q", "shell", "summon", PLUGIN_ID]
}

function opensHome(verb) {
    return HOME_VERBS.indexOf(String(verb)) >= 0
}
