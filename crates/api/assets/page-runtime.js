/*! page-runtime.js — renders a Boma page document into the sandbox document (REQ-039).
 *
 * The document this script runs in is served under a strict content policy:
 *   default-src 'none'; sandbox allow-scripts; script-src 'self'; style-src 'unsafe-inline';
 *   img-src data: https:; connect-src 'none'; form-action 'none'; base-uri 'none'
 *
 * There is no inline script anywhere in the page, no network from this file, and every
 * element is built with textContent or createElement, so a block can never become
 * markup. The blocks are already validated on the way in (boma_domain::page);
 * this renderer is the second and last wall, not the first.
 */
(function () {
  "use strict";

  var dataElement = document.getElementById("page-data");
  if (!dataElement) return;
  var document_;
  try {
    document_ = JSON.parse(dataElement.textContent || "{}");
  } catch (error) {
    return;
  }

  var root = document.getElementById("app");
  if (!root) return;

  function el(tag, className, text) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined && text !== null) node.textContent = String(text);
    return node;
  }

  function twoDecimal(value) {
    var n = Number(value);
    return Number.isFinite(n) ? Math.round(n * 100) / 100 : 0;
  }

  function renderText(block) {
    var p = el("p", "block block-text");
    p.textContent = block.text == null ? "" : String(block.text);
    return p;
  }

  function renderTable(block) {
    var table = el("table", "block block-table");
    var thead = el("thead");
    var headRow = el("tr");
    (block.columns || []).forEach(function (column) {
      headRow.appendChild(el("th", null, column));
    });
    thead.appendChild(headRow);
    table.appendChild(thead);
    var tbody = el("tbody");
    (block.rows || []).forEach(function (row) {
      var tr = el("tr");
      (row || []).forEach(function (cell) {
        tr.appendChild(el("td", null, cell));
      });
      tbody.appendChild(tr);
    });
    table.appendChild(tbody);
    return table;
  }

  function renderChart(block) {
    var maximum = 0;
    (block.points || []).forEach(function (point) {
      maximum = Math.max(maximum, Math.abs(twoDecimal(point.value)));
    });
    var section = el("section", "block block-chart");
    section.appendChild(el("h2", "chart-label", block.label));
    var list = el("ul", "chart-bars");
    (block.points || []).forEach(function (point) {
      var li = el("li", "chart-bar-row");
      li.appendChild(el("span", "chart-bar-label", point.label));
      var bar = el("span", "chart-bar");
      var width = maximum === 0 ? 0 : Math.round((Math.abs(twoDecimal(point.value)) / maximum) * 100);
      bar.style.width = width + "%";
      bar.setAttribute("aria-hidden", "true");
      li.appendChild(bar);
      li.appendChild(el("span", "chart-bar-value", twoDecimal(point.value)));
      list.appendChild(li);
    });
    section.appendChild(list);
    return section;
  }

  function renderLink(block) {
    var a = el("a", "block block-link", block.label);
    a.setAttribute("href", block.href == null ? "#" : String(block.href));
    a.setAttribute("rel", "noopener noreferrer");
    return a;
  }

  function renderImage(block) {
    var img = el("img", "block block-image");
    img.setAttribute("src", block.src == null ? "" : String(block.src));
    img.setAttribute("alt", block.alt == null ? "" : String(block.alt));
    return img;
  }

  var renderers = {
    text: renderText,
    table: renderTable,
    chart: renderChart,
    link: renderLink,
    image: renderImage,
  };

  (document_.blocks || []).forEach(function (block) {
    var render = renderers[block && block.type];
    if (render) root.appendChild(render(block));
  });
})();
