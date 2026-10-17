.pragma library
// Canonical age rules. tools/ui/gen_plugin_strings.py mirrors this dependency-free
// helper into the separately packaged bar plugin.

// Wall age belongs to this life, independently of care, work, XP and appearance.
function age(life, now) {
    var born = life && life.adopted_at_ms
    if ((typeof born !== "number" || !isFinite(born)) || born <= 0 || born > 8640000000000000 || Math.floor(born) !== born
            || (typeof now !== "number" || !isFinite(now)) || now <= 0 || now > 8640000000000000)
        return { known: false, value: null, unit: "", born: null }
    var minutes = Math.floor(Math.max(0, now - born) / 60000)
    var value = minutes >= 1440 ? Math.floor(minutes / 1440) : minutes >= 60 ? Math.floor(minutes / 60) : minutes
    var unit = minutes >= 1440 ? "day" : minutes >= 60 ? "hour" : "minute"
    return { known: true, value: value, unit: unit + (value === 1 ? "" : "s"), born: born }
}

function shortText(life, now) {
    var value = age(life, now)
    if (!value.known) return "Age unavailable"
    if (value.value === 0) return "<1 minute old"
    return value.value + " " + value.unit + " old"
}
