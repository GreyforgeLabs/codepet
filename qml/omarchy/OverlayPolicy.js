.pragma library

// Kept independent of Quickshell so monitor/hit-region policy can be tested.
function selectScreen(screens, preferredName) {
    if (!screens || screens.length === 0)
        return null
    for (var i = 0; i < screens.length; ++i) {
        if (screens[i] && screens[i].name === preferredName)
            return screens[i]
    }
    // The compositor's first connected screen is the documented fallback.
    return screens[0]
}

function clipHit(rect, width, height, active) {
    var empty = { x: 0, y: 0, width: 0, height: 0 }
    if (!active || !rect || !isFinite(rect.x) || !isFinite(rect.y)
            || !isFinite(rect.width) || !isFinite(rect.height)
            || !isFinite(width) || !isFinite(height)
            || rect.width <= 0 || rect.height <= 0 || width <= 0 || height <= 0)
        return empty
    var left = Math.max(0, Math.floor(rect.x))
    var top = Math.max(0, Math.floor(rect.y))
    var right = Math.min(Math.ceil(width), Math.ceil(rect.x + rect.width))
    var bottom = Math.min(Math.ceil(height), Math.ceil(rect.y + rect.height))
    if (right <= left || bottom <= top)
        return empty
    return { x: left, y: top, width: right - left, height: bottom - top }
}
