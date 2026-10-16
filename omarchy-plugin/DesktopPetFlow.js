.pragma library

// "Pet on my desktop" for the bar plugin (SPEC-v0.3 §3.1, §7.2 decision 5, §8). Pure: no
// I/O, no Quickshell. The bar widget writes the returned settings with
// bar.shell.updateEntryInline() and runs the returned launcher verb (codepet-overlay)
// itself; nothing here launches anything. Tested by qml/tests/test_desktop_pet_flow.mjs.
//
// The switch is the existing `overlayOnLoad` setting. The desktop pet process
// (qml/shell.qml) reads the same setting read-only and exits by itself when it is off or
// when CodePet is Off, so turning things off needs no verb; turning them on needs `show`.
//
//   Let's go! (pet.start ok)        -> afterStart(settings)            {settings, launch: "show"}
//   right-click "Pet on my desktop" -> setDesktopPet(settings, on)     {settings, launch: "show"|null}
//   bar load (the leader's startup) -> launchOnLoad(settings, barState) "show" | null
//   CodePet turned back On          -> launchOnEnabled(settings, was, now) "show" | null

var SETTING = "overlayOnLoad"

function isObject(v) {
    return v !== null && typeof v === "object" && !Array.isArray(v)
}

// A copy of the entry's settings without `id` (updateEntryInline adds it back).
function copySettings(settings) {
    var out = {}
    if (isObject(settings)) {
        for (var k in settings)
            if (k !== "id") out[k] = settings[k]
    }
    return out
}

function desktopPetOn(settings) {
    return isObject(settings) && String(settings[SETTING]) === "true"
}

// The settings with the switch set; `changed` is false when nothing needs writing.
function withDesktopPet(settings, on) {
    var next = copySettings(settings)
    var changed = next[SETTING] !== (on === true)
    next[SETTING] = on === true
    return { settings: next, changed: changed }
}

// After a successful Let's go!: the desktop pet is on (decision 5) and appears now.
function afterStart(settings) {
    var w = withDesktopPet(settings, true)
    return { settings: w.settings, changed: w.changed, launch: "show" }
}

// The right-click switch. Off needs no verb: the running pet sees the setting and exits.
function setDesktopPet(settings, on) {
    var w = withDesktopPet(settings, on === true)
    return { settings: w.settings, changed: w.changed, launch: on === true ? "show" : null }
}

// The leader's startup: show the pet when the switch is on, CodePet is not Off and a
// pet has been started (an unarmed or missing life has nothing to show yet).
function launchOnLoad(settings, barState) {
    if (!desktopPetOn(settings) || !isObject(barState) || barState.enabled === false)
        return null
    var life = isObject(barState.life) ? barState.life : null
    return life !== null && life.armed === true ? "show" : null
}

// CodePet turned back On (from the popup, the menu or a click on the Off cell).
function launchOnEnabled(settings, wasEnabled, enabled) {
    return wasEnabled === false && enabled === true && desktopPetOn(settings) ? "show" : null
}
