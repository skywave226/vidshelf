// ArtPlayer 胶水：离线本地播放引擎（默认变体）。
// 经 __TAURI__.core.invoke 复用后端进度/音量持久化，与 libmpv 变体行为一致。
(function () {
  "use strict";
  var instances = new Map();

  function invoke(cmd, args) {
    return window.__TAURI__.core.invoke(cmd, args || {});
  }

  function fmtDuration(ms) {
    var s = Math.floor(ms / 1000);
    var h = Math.floor(s / 3600);
    var m = Math.floor((s % 3600) / 60);
    var sec = s % 60;
    function pad(n) { return n < 10 ? "0" + n : "" + n; }
    return h > 0 ? h + ":" + pad(m) + ":" + pad(sec) : m + ":" + pad(sec);
  }

  function destroy(container) {
    var rec = instances.get(container);
    if (!rec) return;
    instances.delete(container);
    try { rec.save(0); } catch (e) {}
    try { rec.art.destroy(); } catch (e) {}
  }

  function create(container, url, videoId, onError) {
    if (!container || !container.isConnected) return;
    if (typeof window.Artplayer === "undefined") {
      if (onError) onError();
      return;
    }
    Promise.all([
      invoke("get_player_volume").catch(function () { return null; }),
      invoke("get_progress", { videoId: videoId }).catch(function () { return null; })
    ]).then(function (res) {
      if (!container.isConnected) return;
      var pv = res[0] || {};
      var vol = typeof pv.volume === "number" ? pv.volume / 100 : 1;
      var muted = !!pv.mute;
      var p = res[1];
      var resumeMs = p && typeof p.position_ms === "number" ? p.position_ms : 0;

      var art = new window.Artplayer({
        container: container,
        url: url,
        volume: vol,
        muted: muted,
        autoplay: true,
        pip: false,
        setting: true,
        playbackRate: true,
        fullscreen: true,
        fullscreenWeb: true,
        miniProgressBar: true,
        mutex: true,
        backdrop: true,
        playsInline: true,
        autoPlayback: false,
        screenshot: false,
        loop: false,
        theme: "#4f8cff",
        lang: "zh-cn"
      });

      var lastSaved = -1;
      function save(minDelta) {
        var pos = Math.round((art.currentTime || 0) * 1000);
        if (pos > 0 && Math.abs(pos - lastSaved) >= minDelta) {
          lastSaved = pos;
          invoke("save_progress", { videoId: videoId, positionMs: pos }).catch(function () {});
        }
      }
      instances.set(container, { art: art, save: save });

      art.on("ready", function () {
        try {
          var dur = (art.duration || 0) * 1000;
          if (resumeMs > 3000 && dur > 0 && resumeMs < dur - 5000) {
            art.currentTime = resumeMs / 1000;
            if (art.notice) art.notice.show = "已从 " + fmtDuration(resumeMs) + " 继续播放";
          }
        } catch (e) {}
      });
      art.on("video:timeupdate", function () { save(5000); });
      art.on("video:pause", function () { save(1000); });
      art.on("video:ended", function () { save(0); });
      art.on("video:volumechange", function () {
        invoke("save_player_volume", {
          volume: Math.round((art.volume || 0) * 100),
          mute: !!art.muted
        }).catch(function () {});
      });
      art.on("video:error", function () {
        destroy(container);
        if (onError) onError();
      });
    }).catch(function () {
      if (onError) onError();
    });
  }

  window.__vmArt = { create: create, destroy: destroy };
})();