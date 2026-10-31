// ruleid: fold-no-unload
window.addEventListener("unload", () => save());
// ruleid: fold-no-unload
window.onbeforeunload = () => save();
// ok: fold-no-unload
window.addEventListener("pagehide", () => save());

row.addEventListener("click", (e) => {
  // ruleid: fold-click-for-link
  location.href = "/fold/" + id;
});
btn.onclick = function () {
  // ruleid: fold-click-for-link
  window.open(url);
};
// ok: fold-click-for-link
btn.addEventListener("click", () => sign(id));

// ruleid: fold-no-shadow-dom
host.attachShadow({ mode: "open" });

// ruleid: fold-row-about
li.setAttribute("about", iri);
// ok: fold-row-about
span.setAttribute("property", "schema:about");

// ruleid: fold-colour-literal-outside-tokens
el.style.color = "#5C6B63";
// ok: fold-colour-literal-outside-tokens
el.style.color = "var(--soft)";
