pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model

// A painted icon from this plugin's assets/icons/ (SPEC-v0.3 §4). `icon` is
// "<sheet>/<id>", e.g. "care-floor/food". The relative URL resolves against the plugin
// file that declares the Icon (every Icon is declared inside this folder), so icons load
// from the plugin folder; nothing is ever written there. A missing file draws nothing.
Image {
    id: img
    property string icon: ""
    property real size: 48
    width: size
    height: size
    source: Model.iconPath(img.icon)
    sourceSize.width: Math.max(64, Math.ceil(size * 2))
    sourceSize.height: Math.max(64, Math.ceil(size * 2))
    fillMode: Image.PreserveAspectFit
    smooth: true
    mipmap: true
    asynchronous: true
    cache: true
    visible: img.icon !== "" && status !== Image.Error
}
