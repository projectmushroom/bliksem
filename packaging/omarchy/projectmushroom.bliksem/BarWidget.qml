import QtQuick
import Quickshell
import Quickshell.Services.Mpris
import qs.Commons
import qs.Ui

// The bolt on the bar. Track and state come from MPRIS, so the bar never polls the player.
BarWidget {
  id: root
  moduleName: "projectmushroom.bliksem"

  readonly property int maxTitle: setting("maxTitle", 20)
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
  readonly property bool scrolls: playing && !vertical && maxTitle > 0 && track.length > maxTitle
  readonly property string lap: track + "   "
  property int offset: 0
  readonly property string shown: {
    if (!playing || vertical || maxTitle <= 0) return ""
    if (!scrolls) return track
    if (offset === 0) return track.slice(0, maxTitle - 1) + "…"
    return (lap + lap).substr(offset, maxTitle)
  }

  onTrackChanged: {
    slide.stop()
    offset = 0
  }

  // Sits still most of the time: one lap every half minute, and the tooltip always has the whole name.
  Timer {
    id: rest
    interval: 30000
    repeat: true
    running: root.scrolls && !slide.running
    onTriggered: slide.start()
  }

  Timer {
    id: slide
    interval: 160
    repeat: true
    onTriggered: {
      root.offset = (root.offset + 1) % root.lap.length
      if (root.offset === 0) stop()
    }
  }

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
    // While sliding, the width comes from the whole name: the bar's font is proportional, so each slice measures differently.
    fixedWidth: root.shown === "" ? (root.vertical ? -1 : slotSize) : canvasSize + Style.space(16) + (root.scrolls ? labelMetrics.advanceWidth * root.maxTitle / root.track.length : labelMetrics.advanceWidth)
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
      text: root.scrolls ? root.track : root.shown
    }
  }
}
