pragma ComponentBehavior: Bound
import QtQuick

// The Codex picture guide (SPEC-v0.3 §2.3 step 4): one picture per screen the grown-up will
// see in Codex, with one short line under it. An adult page (K1/K2 exempt, K8 applies).
// `kind` "codex.trust": five pages (codex.step1..5); "codex.off": the hooks-off page with
// Try again. The pictures are drawn here (a terminal sketch), no screenshots are shipped.
Item {
    id: guide
    required property var app
    property string kind: "codex.trust"
    property int page: 0
    readonly property var t: app.t
    readonly property bool trust: kind !== "codex.off"
    readonly property int pages: trust ? 5 : 1
    signal pageRequested(int page)
    signal done
    signal retry

    implicitWidth: t.u(448)
    implicitHeight: column.implicitHeight

    // Terminal lines per page: [text, highlighted].
    readonly property var pictures: [
        [["$ codex", true], ["", false], ["", false]],
        [["Some hooks need review", false], ["› Review hooks   (Enter)", true], ["  Skip", false]],
        [["› /hooks", true], ["", false], ["", false]],
        [["CodePet  UserPromptSubmit   ✓", true], ["CodePet  PermissionRequest  ✓", true], ["CodePet  PostToolUse · Stop · …", true]],
        [["Hooks", false], ["  6 trusted", false], ["(Esc) back", true]]
    ]
    readonly property var offLines: [["[features]", false], ["hooks = true", true], ["", false]]

    Column {
        id: column
        width: parent.width
        spacing: guide.t.u(10)

        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: guide.app.s("codex.title")
            textFormat: Text.PlainText
            font.family: guide.t.family
            font.pixelSize: guide.t.fontHeading
            font.bold: true
            color: guide.t.text
        }
        Rectangle {
            objectName: "cp.codexPicture"
            width: parent.width
            height: guide.t.u(180)
            radius: guide.t.smallRadius
            color: "#16181d"
            border.width: 1
            border.color: guide.t.tileBorder
            Row {
                x: guide.t.u(12)
                y: guide.t.u(10)
                spacing: guide.t.u(6)
                Repeater {
                    model: ["#e06c75", "#e5c07b", "#98c379"]
                    Rectangle {
                        required property string modelData
                        width: guide.t.u(12); height: width; radius: width / 2; color: modelData
                    }
                }
            }
            Column {
                x: guide.t.u(18)
                y: guide.t.u(40)
                width: parent.width - guide.t.u(36)
                spacing: guide.t.u(8)
                Repeater {
                    model: guide.trust ? guide.pictures[Math.max(0, Math.min(4, guide.page))] : guide.offLines
                    Rectangle {
                        id: termLine
                        required property var modelData
                        width: parent.width
                        height: guide.t.u(34)
                        radius: guide.t.u(6)
                        color: termLine.modelData[1] ? Qt.rgba(guide.t.accent.r, guide.t.accent.g, guide.t.accent.b, 0.25) : "transparent"
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            x: guide.t.u(8)
                            text: termLine.modelData[0]
                            textFormat: Text.PlainText
                            font.family: "monospace"
                            font.pixelSize: guide.t.fontTitle
                            color: "#e6e6e6"
                        }
                    }
                }
            }
        }
        Repeater {
            model: guide.trust ? ["codex.step" + (guide.page + 1)] : ["codex.off1", "codex.off2", "codex.off3"]
            Text {
                required property string modelData
                width: column.width
                horizontalAlignment: Text.AlignHCenter
                text: guide.app.s(modelData)
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.family: guide.t.family
                font.pixelSize: guide.t.fontHeading
                font.bold: true
                color: guide.t.text
            }
        }
        // Page dots: a picture for each step, the current one lit.
        Row {
            visible: guide.trust
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: guide.t.u(8)
            Repeater {
                model: guide.pages
                Rectangle {
                    required property int index
                    width: guide.t.u(12); height: width; radius: width / 2
                    color: index === guide.page ? guide.t.accent : guide.t.tileBorder
                }
            }
        }
        BigButton {
            objectName: "cp.guideNext"
            visible: guide.trust
            t: guide.t
            width: parent.width
            text: guide.page + 1 < guide.pages ? guide.app.s("btn.next") : guide.app.s("btn.done")
            onTapped: guide.page + 1 < guide.pages ? guide.pageRequested(guide.page + 1) : guide.done()
        }
        BigButton {
            objectName: "cp.guidePrev"
            visible: guide.trust && guide.page > 0
            t: guide.t
            kind: "secondary"
            width: parent.width
            text: "‹ " + guide.app.s("btn.back")
            onTapped: guide.pageRequested(guide.page - 1)
        }
        BigButton {
            objectName: "cp.guideRetry"
            visible: !guide.trust
            t: guide.t
            width: parent.width
            text: guide.app.s("btn.retry")
            onTapped: guide.retry()
        }
        BigButton {
            objectName: "cp.guideClose"
            visible: !guide.trust
            t: guide.t
            kind: "secondary"
            width: parent.width
            text: guide.app.s("btn.close")
            onTapped: guide.done()
        }
    }
}
