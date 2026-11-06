import QtQuick
import Quickshell
import Quickshell.Services.Mpris
import qs.Commons
import qs.Ui

// The bolt on the bar. Track and state come from MPRIS, so the bar never polls the player.
BarWidget {
  id: root
  moduleName: "projectmushroom.bliksem"

  readonly property int maxTitle: setting("maxTitle", 32)
  readonly property var players: Mpris.players ? Mpris.players.values : []
  readonly property var player: {
    for (var i = 0; i < players.length; i++)
      if (players[i] && players[i].identity === "BLIKSEM") return players[i]
    return null
  }
  readonly property bool playing: !!player && player.isPlaying
  readonly property string track: {
    if (!player) return ""
    var artist = player.trackArtist || ""
    var title = player.trackTitle || ""
    return artist && title ? artist + " - " + title : title
  }
  readonly property string shown: playing && !vertical && maxTitle > 0
    ? (track.length > maxTitle ? track.slice(0, maxTitle - 1) + "…" : track) : ""

  function run(word) {
    // The player ships inside the plugin, where `omarchy plugin add` always puts it.
    Quickshell.execDetached([Quickshell.env("HOME") + "/.config/omarchy/plugins/projectmushroom.bliksem/bin/bliksem", word])
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "󱐋"
    labelText: root.shown
    fixedWidth: root.shown === "" ? (root.vertical ? -1 : slotSize) : canvasSize + Style.space(16) + labelMetrics.advanceWidth
    fixedHeight: root.vertical ? slotSize : -1
    active: root.playing
    activeColor: "#ffa81f" // the phosphor, whatever the desktop theme is doing
    tooltipText: root.track || "BLIKSEM"

    onPressed: function(b) {
      if (b === Qt.MiddleButton) root.run("toggle")
      else if (b === Qt.RightButton) root.run("like")
      else root.run("window")
    }
    onWheelMoved: function(delta) {
      root.run(delta > 0 ? "prev" : "next")
    }

    TextMetrics {
      id: labelMetrics
      font.family: button.fontFamily
      font.pixelSize: button.fontSize
      text: root.shown
    }
  }
}
