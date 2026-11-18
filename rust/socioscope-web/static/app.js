(function () {
  var theme = document.getElementById("theme");
  if (theme) {
    theme.addEventListener("click", function () {
      var dark = document.documentElement.classList.toggle("dark");
      localStorage.setItem("socioscope-theme", dark ? "dark" : "light");
    });
  }

  function take() {
    return document.getElementById("take");
  }

  function paint() {
    var el = take();
    if (!el) return;
    var ms = el.currentTime * 1000;
    var active = null;
    document.querySelectorAll(".cue").forEach(function (cue) {
      var start = Number(cue.dataset.start);
      var end = Number(cue.dataset.end);
      var on = ms >= start && ms < end;
      cue.classList.toggle("is-active", on);
      if (on) active = cue;
    });
    if (active) {
      var scroller = document.getElementById("cues");
      if (scroller) {
        var top = active.offsetTop;
        var bottom = top + active.offsetHeight;
        if (top < scroller.scrollTop || bottom > scroller.scrollTop + scroller.clientHeight) {
          active.scrollIntoView({ block: "nearest" });
        }
      }
    }
  }

  document.addEventListener("timeupdate", paint, true);

  document.addEventListener("click", function (event) {
    var bar = event.target.closest("[data-seek]");
    if (!bar) return;
    var el = take();
    if (!el) return;
    el.currentTime = Number(bar.dataset.seek) / 1000;
    el.play();
  });

  document.addEventListener("change", function (event) {
    var input = event.target;
    if (input.matches && input.matches("input[data-autosubmit]") && input.form) {
      if (input.form.requestSubmit) input.form.requestSubmit();
      else input.form.submit();
    }
  });

  document.body.addEventListener("htmx:afterSwap", function () {
    var active = document.querySelector("#cues .cue.is-active");
    if (active) active.scrollIntoView({ block: "nearest" });
  });
})();
