.pragma library
// The helper owns version comparison, downloads and installation. This model only
// validates its bounded display response and selects the page action.
var STATES = ["idle", "checking", "available", "current", "downloading", "verifying", "installing", "succeeded", "error"]
function active(value) {
    return !!value && ["checking", "downloading", "verifying", "installing"].indexOf(value.state) >= 0
}
function version(value) { return typeof value === "string" && (value === "" || /^\d+\.\d+\.\d+$/.test(value)) }
function parse(text) {
    if (typeof text !== "string" || text.length > 32768) return null
    var value
    try { value = JSON.parse(text) } catch (e) { return null }
    if (!value || value.schema !== "codepet.update.v1" || STATES.indexOf(value.state) < 0
            || !version(value.current) || !version(value.latest)
            || typeof value.message !== "string" || value.message.length > 4096
            || typeof value.checked_at !== "number" || !isFinite(value.checked_at) || value.checked_at < 0) return null
    return value
}
function canInstall(value, busy) { return !busy && !!value && value.state === "available" && value.latest !== "" }
