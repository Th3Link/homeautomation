var __create = Object.create;
var __freeze = Object.freeze;
var __defProp = Object.defineProperty;
var __defProps = Object.defineProperties;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropDescs = Object.getOwnPropertyDescriptors;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getOwnPropSymbols = Object.getOwnPropertySymbols;
var __getProtoOf = Object.getPrototypeOf;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __propIsEnum = Object.prototype.propertyIsEnumerable;
var __defNormalProp = (obj, key, value) => key in obj ? __defProp(obj, key, { enumerable: true, configurable: true, writable: true, value }) : obj[key] = value;
var __spreadValues = (a3, b) => {
  for (var prop in b || (b = {}))
    if (__hasOwnProp.call(b, prop))
      __defNormalProp(a3, prop, b[prop]);
  if (__getOwnPropSymbols)
    for (var prop of __getOwnPropSymbols(b)) {
      if (__propIsEnum.call(b, prop))
        __defNormalProp(a3, prop, b[prop]);
    }
  return a3;
};
var __spreadProps = (a3, b) => __defProps(a3, __getOwnPropDescs(b));
var __name = (target, value) => __defProp(target, "name", { value, configurable: true });
var __esm = (fn, res) => function __init() {
  return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
};
var __commonJS = (cb, mod) => function __require() {
  return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
  mod
));
var __template = (cooked, raw) => __freeze(__defProp(cooked, "raw", { value: __freeze(raw || cooked.slice()) }));

// node_modules/preact/dist/preact.module.js
function d(n3, l3) {
  for (var u3 in l3) n3[u3] = l3[u3];
  return n3;
}
function g(n3) {
  n3 && n3.parentNode && n3.parentNode.removeChild(n3);
}
function _(l3, u3, t4) {
  var i3, r3, o3, e3 = {};
  for (o3 in u3) "key" == o3 ? i3 = u3[o3] : "ref" == o3 ? r3 = u3[o3] : e3[o3] = u3[o3];
  if (arguments.length > 2 && (e3.children = arguments.length > 3 ? n.call(arguments, 2) : t4), "function" == typeof l3 && null != l3.defaultProps) for (o3 in l3.defaultProps) void 0 === e3[o3] && (e3[o3] = l3.defaultProps[o3]);
  return m(l3, e3, i3, r3, null);
}
function m(n3, t4, i3, r3, o3) {
  var e3 = { type: n3, props: t4, key: i3, ref: r3, __k: null, __: null, __b: 0, __e: null, __c: null, constructor: void 0, __v: null == o3 ? ++u : o3, __i: -1, __u: 0 };
  return null == o3 && null != l.vnode && l.vnode(e3), e3;
}
function k(n3) {
  return n3.children;
}
function x(n3, l3) {
  this.props = n3, this.context = l3;
}
function S(n3, l3) {
  if (null == l3) return n3.__ ? S(n3.__, n3.__i + 1) : null;
  for (var u3; l3 < n3.__k.length; l3++) if (null != (u3 = n3.__k[l3]) && null != u3.__e) return u3.__e;
  return "function" == typeof n3.type ? S(n3) : null;
}
function C(n3) {
  var l3, u3;
  if (null != (n3 = n3.__) && null != n3.__c) {
    for (n3.__e = n3.__c.base = null, l3 = 0; l3 < n3.__k.length; l3++) if (null != (u3 = n3.__k[l3]) && null != u3.__e) {
      n3.__e = n3.__c.base = u3.__e;
      break;
    }
    return C(n3);
  }
}
function M(n3) {
  (!n3.__d && (n3.__d = true) && i.push(n3) && !$.__r++ || r != l.debounceRendering) && ((r = l.debounceRendering) || o)($);
}
function $() {
  for (var n3, u3, t4, r3, o3, f3, c3, s3 = 1; i.length; ) i.length > s3 && i.sort(e), n3 = i.shift(), s3 = i.length, n3.__d && (t4 = void 0, o3 = (r3 = (u3 = n3).__v).__e, f3 = [], c3 = [], u3.__P && ((t4 = d({}, r3)).__v = r3.__v + 1, l.vnode && l.vnode(t4), O(u3.__P, t4, r3, u3.__n, u3.__P.namespaceURI, 32 & r3.__u ? [o3] : null, f3, null == o3 ? S(r3) : o3, !!(32 & r3.__u), c3), t4.__v = r3.__v, t4.__.__k[t4.__i] = t4, z(f3, t4, c3), t4.__e != o3 && C(t4)));
  $.__r = 0;
}
function I(n3, l3, u3, t4, i3, r3, o3, e3, f3, c3, s3) {
  var a3, h3, y3, w3, d3, g2, _2 = t4 && t4.__k || v, m3 = l3.length;
  for (f3 = P(u3, l3, _2, f3, m3), a3 = 0; a3 < m3; a3++) null != (y3 = u3.__k[a3]) && (h3 = -1 == y3.__i ? p : _2[y3.__i] || p, y3.__i = a3, g2 = O(n3, y3, h3, i3, r3, o3, e3, f3, c3, s3), w3 = y3.__e, y3.ref && h3.ref != y3.ref && (h3.ref && q(h3.ref, null, y3), s3.push(y3.ref, y3.__c || w3, y3)), null == d3 && null != w3 && (d3 = w3), 4 & y3.__u || h3.__k === y3.__k ? f3 = A(y3, f3, n3) : "function" == typeof y3.type && void 0 !== g2 ? f3 = g2 : w3 && (f3 = w3.nextSibling), y3.__u &= -7);
  return u3.__e = d3, f3;
}
function P(n3, l3, u3, t4, i3) {
  var r3, o3, e3, f3, c3, s3 = u3.length, a3 = s3, h3 = 0;
  for (n3.__k = new Array(i3), r3 = 0; r3 < i3; r3++) null != (o3 = l3[r3]) && "boolean" != typeof o3 && "function" != typeof o3 ? (f3 = r3 + h3, (o3 = n3.__k[r3] = "string" == typeof o3 || "number" == typeof o3 || "bigint" == typeof o3 || o3.constructor == String ? m(null, o3, null, null, null) : w(o3) ? m(k, { children: o3 }, null, null, null) : null == o3.constructor && o3.__b > 0 ? m(o3.type, o3.props, o3.key, o3.ref ? o3.ref : null, o3.__v) : o3).__ = n3, o3.__b = n3.__b + 1, e3 = null, -1 != (c3 = o3.__i = L(o3, u3, f3, a3)) && (a3--, (e3 = u3[c3]) && (e3.__u |= 2)), null == e3 || null == e3.__v ? (-1 == c3 && (i3 > s3 ? h3-- : i3 < s3 && h3++), "function" != typeof o3.type && (o3.__u |= 4)) : c3 != f3 && (c3 == f3 - 1 ? h3-- : c3 == f3 + 1 ? h3++ : (c3 > f3 ? h3-- : h3++, o3.__u |= 4))) : n3.__k[r3] = null;
  if (a3) for (r3 = 0; r3 < s3; r3++) null != (e3 = u3[r3]) && 0 == (2 & e3.__u) && (e3.__e == t4 && (t4 = S(e3)), B(e3, e3));
  return t4;
}
function A(n3, l3, u3) {
  var t4, i3;
  if ("function" == typeof n3.type) {
    for (t4 = n3.__k, i3 = 0; t4 && i3 < t4.length; i3++) t4[i3] && (t4[i3].__ = n3, l3 = A(t4[i3], l3, u3));
    return l3;
  }
  n3.__e != l3 && (l3 && n3.type && !u3.contains(l3) && (l3 = S(n3)), u3.insertBefore(n3.__e, l3 || null), l3 = n3.__e);
  do {
    l3 = l3 && l3.nextSibling;
  } while (null != l3 && 8 == l3.nodeType);
  return l3;
}
function L(n3, l3, u3, t4) {
  var i3, r3, o3 = n3.key, e3 = n3.type, f3 = l3[u3];
  if (null === f3 && null == n3.key || f3 && o3 == f3.key && e3 == f3.type && 0 == (2 & f3.__u)) return u3;
  if (t4 > (null != f3 && 0 == (2 & f3.__u) ? 1 : 0)) for (i3 = u3 - 1, r3 = u3 + 1; i3 >= 0 || r3 < l3.length; ) {
    if (i3 >= 0) {
      if ((f3 = l3[i3]) && 0 == (2 & f3.__u) && o3 == f3.key && e3 == f3.type) return i3;
      i3--;
    }
    if (r3 < l3.length) {
      if ((f3 = l3[r3]) && 0 == (2 & f3.__u) && o3 == f3.key && e3 == f3.type) return r3;
      r3++;
    }
  }
  return -1;
}
function T(n3, l3, u3) {
  "-" == l3[0] ? n3.setProperty(l3, null == u3 ? "" : u3) : n3[l3] = null == u3 ? "" : "number" != typeof u3 || y.test(l3) ? u3 : u3 + "px";
}
function j(n3, l3, u3, t4, i3) {
  var r3, o3;
  n: if ("style" == l3) if ("string" == typeof u3) n3.style.cssText = u3;
  else {
    if ("string" == typeof t4 && (n3.style.cssText = t4 = ""), t4) for (l3 in t4) u3 && l3 in u3 || T(n3.style, l3, "");
    if (u3) for (l3 in u3) t4 && u3[l3] == t4[l3] || T(n3.style, l3, u3[l3]);
  }
  else if ("o" == l3[0] && "n" == l3[1]) r3 = l3 != (l3 = l3.replace(f, "$1")), o3 = l3.toLowerCase(), l3 = o3 in n3 || "onFocusOut" == l3 || "onFocusIn" == l3 ? o3.slice(2) : l3.slice(2), n3.l || (n3.l = {}), n3.l[l3 + r3] = u3, u3 ? t4 ? u3.u = t4.u : (u3.u = c, n3.addEventListener(l3, r3 ? a : s, r3)) : n3.removeEventListener(l3, r3 ? a : s, r3);
  else {
    if ("http://www.w3.org/2000/svg" == i3) l3 = l3.replace(/xlink(H|:h)/, "h").replace(/sName$/, "s");
    else if ("width" != l3 && "height" != l3 && "href" != l3 && "list" != l3 && "form" != l3 && "tabIndex" != l3 && "download" != l3 && "rowSpan" != l3 && "colSpan" != l3 && "role" != l3 && "popover" != l3 && l3 in n3) try {
      n3[l3] = null == u3 ? "" : u3;
      break n;
    } catch (n4) {
    }
    "function" == typeof u3 || (null == u3 || false === u3 && "-" != l3[4] ? n3.removeAttribute(l3) : n3.setAttribute(l3, "popover" == l3 && 1 == u3 ? "" : u3));
  }
}
function F(n3) {
  return function(u3) {
    if (this.l) {
      var t4 = this.l[u3.type + n3];
      if (null == u3.t) u3.t = c++;
      else if (u3.t < t4.u) return;
      return t4(l.event ? l.event(u3) : u3);
    }
  };
}
function O(n3, u3, t4, i3, r3, o3, e3, f3, c3, s3) {
  var a3, h3, p3, v3, y3, _2, m3, b, S2, C3, M2, $2, P2, A3, H, L2, T3, j3 = u3.type;
  if (null != u3.constructor) return null;
  128 & t4.__u && (c3 = !!(32 & t4.__u), o3 = [f3 = u3.__e = t4.__e]), (a3 = l.__b) && a3(u3);
  n: if ("function" == typeof j3) try {
    if (b = u3.props, S2 = "prototype" in j3 && j3.prototype.render, C3 = (a3 = j3.contextType) && i3[a3.__c], M2 = a3 ? C3 ? C3.props.value : a3.__ : i3, t4.__c ? m3 = (h3 = u3.__c = t4.__c).__ = h3.__E : (S2 ? u3.__c = h3 = new j3(b, M2) : (u3.__c = h3 = new x(b, M2), h3.constructor = j3, h3.render = D), C3 && C3.sub(h3), h3.props = b, h3.state || (h3.state = {}), h3.context = M2, h3.__n = i3, p3 = h3.__d = true, h3.__h = [], h3._sb = []), S2 && null == h3.__s && (h3.__s = h3.state), S2 && null != j3.getDerivedStateFromProps && (h3.__s == h3.state && (h3.__s = d({}, h3.__s)), d(h3.__s, j3.getDerivedStateFromProps(b, h3.__s))), v3 = h3.props, y3 = h3.state, h3.__v = u3, p3) S2 && null == j3.getDerivedStateFromProps && null != h3.componentWillMount && h3.componentWillMount(), S2 && null != h3.componentDidMount && h3.__h.push(h3.componentDidMount);
    else {
      if (S2 && null == j3.getDerivedStateFromProps && b !== v3 && null != h3.componentWillReceiveProps && h3.componentWillReceiveProps(b, M2), !h3.__e && null != h3.shouldComponentUpdate && false === h3.shouldComponentUpdate(b, h3.__s, M2) || u3.__v == t4.__v) {
        for (u3.__v != t4.__v && (h3.props = b, h3.state = h3.__s, h3.__d = false), u3.__e = t4.__e, u3.__k = t4.__k, u3.__k.some(function(n4) {
          n4 && (n4.__ = u3);
        }), $2 = 0; $2 < h3._sb.length; $2++) h3.__h.push(h3._sb[$2]);
        h3._sb = [], h3.__h.length && e3.push(h3);
        break n;
      }
      null != h3.componentWillUpdate && h3.componentWillUpdate(b, h3.__s, M2), S2 && null != h3.componentDidUpdate && h3.__h.push(function() {
        h3.componentDidUpdate(v3, y3, _2);
      });
    }
    if (h3.context = M2, h3.props = b, h3.__P = n3, h3.__e = false, P2 = l.__r, A3 = 0, S2) {
      for (h3.state = h3.__s, h3.__d = false, P2 && P2(u3), a3 = h3.render(h3.props, h3.state, h3.context), H = 0; H < h3._sb.length; H++) h3.__h.push(h3._sb[H]);
      h3._sb = [];
    } else do {
      h3.__d = false, P2 && P2(u3), a3 = h3.render(h3.props, h3.state, h3.context), h3.state = h3.__s;
    } while (h3.__d && ++A3 < 25);
    h3.state = h3.__s, null != h3.getChildContext && (i3 = d(d({}, i3), h3.getChildContext())), S2 && !p3 && null != h3.getSnapshotBeforeUpdate && (_2 = h3.getSnapshotBeforeUpdate(v3, y3)), L2 = a3, null != a3 && a3.type === k && null == a3.key && (L2 = N(a3.props.children)), f3 = I(n3, w(L2) ? L2 : [L2], u3, t4, i3, r3, o3, e3, f3, c3, s3), h3.base = u3.__e, u3.__u &= -161, h3.__h.length && e3.push(h3), m3 && (h3.__E = h3.__ = null);
  } catch (n4) {
    if (u3.__v = null, c3 || null != o3) if (n4.then) {
      for (u3.__u |= c3 ? 160 : 128; f3 && 8 == f3.nodeType && f3.nextSibling; ) f3 = f3.nextSibling;
      o3[o3.indexOf(f3)] = null, u3.__e = f3;
    } else for (T3 = o3.length; T3--; ) g(o3[T3]);
    else u3.__e = t4.__e, u3.__k = t4.__k;
    l.__e(n4, u3, t4);
  }
  else null == o3 && u3.__v == t4.__v ? (u3.__k = t4.__k, u3.__e = t4.__e) : f3 = u3.__e = V(t4.__e, u3, t4, i3, r3, o3, e3, c3, s3);
  return (a3 = l.diffed) && a3(u3), 128 & u3.__u ? void 0 : f3;
}
function z(n3, u3, t4) {
  for (var i3 = 0; i3 < t4.length; i3++) q(t4[i3], t4[++i3], t4[++i3]);
  l.__c && l.__c(u3, n3), n3.some(function(u4) {
    try {
      n3 = u4.__h, u4.__h = [], n3.some(function(n4) {
        n4.call(u4);
      });
    } catch (n4) {
      l.__e(n4, u4.__v);
    }
  });
}
function N(n3) {
  return "object" != typeof n3 || null == n3 || n3.__b && n3.__b > 0 ? n3 : w(n3) ? n3.map(N) : d({}, n3);
}
function V(u3, t4, i3, r3, o3, e3, f3, c3, s3) {
  var a3, h3, v3, y3, d3, _2, m3, b = i3.props, k3 = t4.props, x3 = t4.type;
  if ("svg" == x3 ? o3 = "http://www.w3.org/2000/svg" : "math" == x3 ? o3 = "http://www.w3.org/1998/Math/MathML" : o3 || (o3 = "http://www.w3.org/1999/xhtml"), null != e3) {
    for (a3 = 0; a3 < e3.length; a3++) if ((d3 = e3[a3]) && "setAttribute" in d3 == !!x3 && (x3 ? d3.localName == x3 : 3 == d3.nodeType)) {
      u3 = d3, e3[a3] = null;
      break;
    }
  }
  if (null == u3) {
    if (null == x3) return document.createTextNode(k3);
    u3 = document.createElementNS(o3, x3, k3.is && k3), c3 && (l.__m && l.__m(t4, e3), c3 = false), e3 = null;
  }
  if (null == x3) b === k3 || c3 && u3.data == k3 || (u3.data = k3);
  else {
    if (e3 = e3 && n.call(u3.childNodes), b = i3.props || p, !c3 && null != e3) for (b = {}, a3 = 0; a3 < u3.attributes.length; a3++) b[(d3 = u3.attributes[a3]).name] = d3.value;
    for (a3 in b) if (d3 = b[a3], "children" == a3) ;
    else if ("dangerouslySetInnerHTML" == a3) v3 = d3;
    else if (!(a3 in k3)) {
      if ("value" == a3 && "defaultValue" in k3 || "checked" == a3 && "defaultChecked" in k3) continue;
      j(u3, a3, null, d3, o3);
    }
    for (a3 in k3) d3 = k3[a3], "children" == a3 ? y3 = d3 : "dangerouslySetInnerHTML" == a3 ? h3 = d3 : "value" == a3 ? _2 = d3 : "checked" == a3 ? m3 = d3 : c3 && "function" != typeof d3 || b[a3] === d3 || j(u3, a3, d3, b[a3], o3);
    if (h3) c3 || v3 && (h3.__html == v3.__html || h3.__html == u3.innerHTML) || (u3.innerHTML = h3.__html), t4.__k = [];
    else if (v3 && (u3.innerHTML = ""), I("template" == t4.type ? u3.content : u3, w(y3) ? y3 : [y3], t4, i3, r3, "foreignObject" == x3 ? "http://www.w3.org/1999/xhtml" : o3, e3, f3, e3 ? e3[0] : i3.__k && S(i3, 0), c3, s3), null != e3) for (a3 = e3.length; a3--; ) g(e3[a3]);
    c3 || (a3 = "value", "progress" == x3 && null == _2 ? u3.removeAttribute("value") : null != _2 && (_2 !== u3[a3] || "progress" == x3 && !_2 || "option" == x3 && _2 != b[a3]) && j(u3, a3, _2, b[a3], o3), a3 = "checked", null != m3 && m3 != u3[a3] && j(u3, a3, m3, b[a3], o3));
  }
  return u3;
}
function q(n3, u3, t4) {
  try {
    if ("function" == typeof n3) {
      var i3 = "function" == typeof n3.__u;
      i3 && n3.__u(), i3 && null == u3 || (n3.__u = n3(u3));
    } else n3.current = u3;
  } catch (n4) {
    l.__e(n4, t4);
  }
}
function B(n3, u3, t4) {
  var i3, r3;
  if (l.unmount && l.unmount(n3), (i3 = n3.ref) && (i3.current && i3.current != n3.__e || q(i3, null, u3)), null != (i3 = n3.__c)) {
    if (i3.componentWillUnmount) try {
      i3.componentWillUnmount();
    } catch (n4) {
      l.__e(n4, u3);
    }
    i3.base = i3.__P = null;
  }
  if (i3 = n3.__k) for (r3 = 0; r3 < i3.length; r3++) i3[r3] && B(i3[r3], u3, t4 || "function" != typeof n3.type);
  t4 || g(n3.__e), n3.__c = n3.__ = n3.__e = void 0;
}
function D(n3, l3, u3) {
  return this.constructor(n3, u3);
}
function E(u3, t4, i3) {
  var r3, o3, e3, f3;
  t4 == document && (t4 = document.documentElement), l.__ && l.__(u3, t4), o3 = (r3 = "function" == typeof i3) ? null : i3 && i3.__k || t4.__k, e3 = [], f3 = [], O(t4, u3 = (!r3 && i3 || t4).__k = _(k, null, [u3]), o3 || p, p, t4.namespaceURI, !r3 && i3 ? [i3] : o3 ? null : t4.firstChild ? n.call(t4.childNodes) : null, e3, !r3 && i3 ? i3 : o3 ? o3.__e : t4.firstChild, r3, f3), z(e3, u3, f3);
}
function K(n3) {
  function l3(n4) {
    var u3, t4;
    return this.getChildContext || (u3 = /* @__PURE__ */ new Set(), (t4 = {})[l3.__c] = this, this.getChildContext = function() {
      return t4;
    }, this.componentWillUnmount = function() {
      u3 = null;
    }, this.shouldComponentUpdate = function(n5) {
      this.props.value != n5.value && u3.forEach(function(n6) {
        n6.__e = true, M(n6);
      });
    }, this.sub = function(n5) {
      u3.add(n5);
      var l4 = n5.componentWillUnmount;
      n5.componentWillUnmount = function() {
        u3 && u3.delete(n5), l4 && l4.call(n5);
      };
    }), n4.children;
  }
  __name(l3, "l");
  return l3.__c = "__cC" + h++, l3.__ = n3, l3.Provider = l3.__l = (l3.Consumer = function(n4, l4) {
    return n4.children(l4);
  }).contextType = l3, l3;
}
var n, l, u, t, i, r, o, e, f, c, s, a, h, p, v, y, w;
var init_preact_module = __esm({
  "node_modules/preact/dist/preact.module.js"() {
    init_preact_shim();
    p = {};
    v = [];
    y = /acit|ex(?:s|g|n|p|$)|rph|grid|ows|mnc|ntw|ine[ch]|zoo|^ord|itera/i;
    w = Array.isArray;
    __name(d, "d");
    __name(g, "g");
    __name(_, "_");
    __name(m, "m");
    __name(k, "k");
    __name(x, "x");
    __name(S, "S");
    __name(C, "C");
    __name(M, "M");
    __name($, "$");
    __name(I, "I");
    __name(P, "P");
    __name(A, "A");
    __name(L, "L");
    __name(T, "T");
    __name(j, "j");
    __name(F, "F");
    __name(O, "O");
    __name(z, "z");
    __name(N, "N");
    __name(V, "V");
    __name(q, "q");
    __name(B, "B");
    __name(D, "D");
    __name(E, "E");
    __name(K, "K");
    n = v.slice, l = { __e: /* @__PURE__ */ __name(function(n3, l3, u3, t4) {
      for (var i3, r3, o3; l3 = l3.__; ) if ((i3 = l3.__c) && !i3.__) try {
        if ((r3 = i3.constructor) && null != r3.getDerivedStateFromError && (i3.setState(r3.getDerivedStateFromError(n3)), o3 = i3.__d), null != i3.componentDidCatch && (i3.componentDidCatch(n3, t4 || {}), o3 = i3.__d), o3) return i3.__E = i3;
      } catch (l4) {
        n3 = l4;
      }
      throw n3;
    }, "__e") }, u = 0, t = /* @__PURE__ */ __name(function(n3) {
      return null != n3 && null == n3.constructor;
    }, "t"), x.prototype.setState = function(n3, l3) {
      var u3;
      u3 = null != this.__s && this.__s != this.state ? this.__s : this.__s = d({}, this.state), "function" == typeof n3 && (n3 = n3(d({}, u3), this.props)), n3 && d(u3, n3), null != n3 && this.__v && (l3 && this._sb.push(l3), M(this));
    }, x.prototype.forceUpdate = function(n3) {
      this.__v && (this.__e = true, n3 && this.__h.push(n3), M(this));
    }, x.prototype.render = k, i = [], o = "function" == typeof Promise ? Promise.prototype.then.bind(Promise.resolve()) : setTimeout, e = /* @__PURE__ */ __name(function(n3, l3) {
      return n3.__v.__b - l3.__v.__b;
    }, "e"), $.__r = 0, f = /(PointerCapture)$|Capture$/i, c = 0, s = F(false), a = F(true), h = 0;
  }
});

// preact-shim.js
var init_preact_shim = __esm({
  "preact-shim.js"() {
    init_preact_module();
  }
});

// node_modules/crc-32/crc32.js
var require_crc32 = __commonJS({
  "node_modules/crc-32/crc32.js"(exports) {
    init_preact_shim();
    var CRC322;
    (function(factory) {
      if (typeof DO_NOT_EXPORT_CRC === "undefined") {
        if ("object" === typeof exports) {
          factory(exports);
        } else if ("function" === typeof define && define.amd) {
          define(function() {
            var module2 = {};
            factory(module2);
            return module2;
          });
        } else {
          factory(CRC322 = {});
        }
      } else {
        factory(CRC322 = {});
      }
    })(function(CRC323) {
      CRC323.version = "1.2.2";
      function signed_crc_table() {
        var c3 = 0, table = new Array(256);
        for (var n3 = 0; n3 != 256; ++n3) {
          c3 = n3;
          c3 = c3 & 1 ? -306674912 ^ c3 >>> 1 : c3 >>> 1;
          c3 = c3 & 1 ? -306674912 ^ c3 >>> 1 : c3 >>> 1;
          c3 = c3 & 1 ? -306674912 ^ c3 >>> 1 : c3 >>> 1;
          c3 = c3 & 1 ? -306674912 ^ c3 >>> 1 : c3 >>> 1;
          c3 = c3 & 1 ? -306674912 ^ c3 >>> 1 : c3 >>> 1;
          c3 = c3 & 1 ? -306674912 ^ c3 >>> 1 : c3 >>> 1;
          c3 = c3 & 1 ? -306674912 ^ c3 >>> 1 : c3 >>> 1;
          c3 = c3 & 1 ? -306674912 ^ c3 >>> 1 : c3 >>> 1;
          table[n3] = c3;
        }
        return typeof Int32Array !== "undefined" ? new Int32Array(table) : table;
      }
      __name(signed_crc_table, "signed_crc_table");
      var T0 = signed_crc_table();
      function slice_by_16_tables(T10) {
        var c3 = 0, v3 = 0, n3 = 0, table = typeof Int32Array !== "undefined" ? new Int32Array(4096) : new Array(4096);
        for (n3 = 0; n3 != 256; ++n3) table[n3] = T10[n3];
        for (n3 = 0; n3 != 256; ++n3) {
          v3 = T10[n3];
          for (c3 = 256 + n3; c3 < 4096; c3 += 256) v3 = table[c3] = v3 >>> 8 ^ T10[v3 & 255];
        }
        var out = [];
        for (n3 = 1; n3 != 16; ++n3) out[n3 - 1] = typeof Int32Array !== "undefined" ? table.subarray(n3 * 256, n3 * 256 + 256) : table.slice(n3 * 256, n3 * 256 + 256);
        return out;
      }
      __name(slice_by_16_tables, "slice_by_16_tables");
      var TT = slice_by_16_tables(T0);
      var T1 = TT[0], T22 = TT[1], T3 = TT[2], T4 = TT[3], T5 = TT[4];
      var T6 = TT[5], T7 = TT[6], T8 = TT[7], T9 = TT[8], Ta = TT[9];
      var Tb = TT[10], Tc = TT[11], Td = TT[12], Te = TT[13], Tf = TT[14];
      function crc32_bstr(bstr, seed) {
        var C3 = seed ^ -1;
        for (var i3 = 0, L2 = bstr.length; i3 < L2; ) C3 = C3 >>> 8 ^ T0[(C3 ^ bstr.charCodeAt(i3++)) & 255];
        return ~C3;
      }
      __name(crc32_bstr, "crc32_bstr");
      function crc32_buf(B3, seed) {
        var C3 = seed ^ -1, L2 = B3.length - 15, i3 = 0;
        for (; i3 < L2; ) C3 = Tf[B3[i3++] ^ C3 & 255] ^ Te[B3[i3++] ^ C3 >> 8 & 255] ^ Td[B3[i3++] ^ C3 >> 16 & 255] ^ Tc[B3[i3++] ^ C3 >>> 24] ^ Tb[B3[i3++]] ^ Ta[B3[i3++]] ^ T9[B3[i3++]] ^ T8[B3[i3++]] ^ T7[B3[i3++]] ^ T6[B3[i3++]] ^ T5[B3[i3++]] ^ T4[B3[i3++]] ^ T3[B3[i3++]] ^ T22[B3[i3++]] ^ T1[B3[i3++]] ^ T0[B3[i3++]];
        L2 += 15;
        while (i3 < L2) C3 = C3 >>> 8 ^ T0[(C3 ^ B3[i3++]) & 255];
        return ~C3;
      }
      __name(crc32_buf, "crc32_buf");
      function crc32_str(str, seed) {
        var C3 = seed ^ -1;
        for (var i3 = 0, L2 = str.length, c3 = 0, d3 = 0; i3 < L2; ) {
          c3 = str.charCodeAt(i3++);
          if (c3 < 128) {
            C3 = C3 >>> 8 ^ T0[(C3 ^ c3) & 255];
          } else if (c3 < 2048) {
            C3 = C3 >>> 8 ^ T0[(C3 ^ (192 | c3 >> 6 & 31)) & 255];
            C3 = C3 >>> 8 ^ T0[(C3 ^ (128 | c3 & 63)) & 255];
          } else if (c3 >= 55296 && c3 < 57344) {
            c3 = (c3 & 1023) + 64;
            d3 = str.charCodeAt(i3++) & 1023;
            C3 = C3 >>> 8 ^ T0[(C3 ^ (240 | c3 >> 8 & 7)) & 255];
            C3 = C3 >>> 8 ^ T0[(C3 ^ (128 | c3 >> 2 & 63)) & 255];
            C3 = C3 >>> 8 ^ T0[(C3 ^ (128 | d3 >> 6 & 15 | (c3 & 3) << 4)) & 255];
            C3 = C3 >>> 8 ^ T0[(C3 ^ (128 | d3 & 63)) & 255];
          } else {
            C3 = C3 >>> 8 ^ T0[(C3 ^ (224 | c3 >> 12 & 15)) & 255];
            C3 = C3 >>> 8 ^ T0[(C3 ^ (128 | c3 >> 6 & 63)) & 255];
            C3 = C3 >>> 8 ^ T0[(C3 ^ (128 | c3 & 63)) & 255];
          }
        }
        return ~C3;
      }
      __name(crc32_str, "crc32_str");
      CRC323.table = T0;
      CRC323.bstr = crc32_bstr;
      CRC323.buf = crc32_buf;
      CRC323.str = crc32_str;
    });
  }
});

// src/js/app.js
init_preact_shim();
init_preact_module();

// node_modules/htm/dist/htm.module.js
init_preact_shim();
var n2 = /* @__PURE__ */ __name(function(t4, s3, r3, e3) {
  var u3;
  s3[0] = 0;
  for (var h3 = 1; h3 < s3.length; h3++) {
    var p3 = s3[h3++], a3 = s3[h3] ? (s3[0] |= p3 ? 1 : 2, r3[s3[h3++]]) : s3[++h3];
    3 === p3 ? e3[0] = a3 : 4 === p3 ? e3[1] = Object.assign(e3[1] || {}, a3) : 5 === p3 ? (e3[1] = e3[1] || {})[s3[++h3]] = a3 : 6 === p3 ? e3[1][s3[++h3]] += a3 + "" : p3 ? (u3 = t4.apply(a3, n2(t4, a3, r3, ["", null])), e3.push(u3), a3[0] ? s3[0] |= 2 : (s3[h3 - 2] = 0, s3[h3] = u3)) : e3.push(a3);
  }
  return e3;
}, "n");
var t2 = /* @__PURE__ */ new Map();
function htm_module_default(s3) {
  var r3 = t2.get(this);
  return r3 || (r3 = /* @__PURE__ */ new Map(), t2.set(this, r3)), (r3 = n2(this, r3.get(s3) || (r3.set(s3, r3 = function(n3) {
    for (var t4, s4, r4 = 1, e3 = "", u3 = "", h3 = [0], p3 = function(n4) {
      1 === r4 && (n4 || (e3 = e3.replace(/^\s*\n\s*|\s*\n\s*$/g, ""))) ? h3.push(0, n4, e3) : 3 === r4 && (n4 || e3) ? (h3.push(3, n4, e3), r4 = 2) : 2 === r4 && "..." === e3 && n4 ? h3.push(4, n4, 0) : 2 === r4 && e3 && !n4 ? h3.push(5, 0, true, e3) : r4 >= 5 && ((e3 || !n4 && 5 === r4) && (h3.push(r4, 0, e3, s4), r4 = 6), n4 && (h3.push(r4, n4, 0, s4), r4 = 6)), e3 = "";
    }, a3 = 0; a3 < n3.length; a3++) {
      a3 && (1 === r4 && p3(), p3(a3));
      for (var l3 = 0; l3 < n3[a3].length; l3++) t4 = n3[a3][l3], 1 === r4 ? "<" === t4 ? (p3(), h3 = [h3], r4 = 3) : e3 += t4 : 4 === r4 ? "--" === e3 && ">" === t4 ? (r4 = 1, e3 = "") : e3 = t4 + e3[0] : u3 ? t4 === u3 ? u3 = "" : e3 += t4 : '"' === t4 || "'" === t4 ? u3 = t4 : ">" === t4 ? (p3(), r4 = 1) : r4 && ("=" === t4 ? (r4 = 5, s4 = e3, e3 = "") : "/" === t4 && (r4 < 5 || ">" === n3[a3][l3 + 1]) ? (p3(), 3 === r4 && (h3 = h3[0]), r4 = h3, (h3 = h3[0]).push(2, 0, r4), r4 = 0) : " " === t4 || "	" === t4 || "\n" === t4 || "\r" === t4 ? (p3(), r4 = 2) : e3 += t4), 3 === r4 && "!--" === e3 && (r4 = 4, h3 = h3[0]);
    }
    return p3(), h3;
  }(s3)), r3), arguments, [])).length > 1 ? r3 : r3[0];
}
__name(htm_module_default, "default");

// src/js/app.js
var import_crc_32 = __toESM(require_crc32());

// src/js/api.js
init_preact_shim();
var CRC32 = __toESM(require_crc32());
var loadDevices = /* @__PURE__ */ __name(async () => {
  const response = await fetch("state.json");
  if (!response.ok) throw new Error("HTTP ".concat(response.status, " - Failed to load devices"));
  return await response.json();
}, "loadDevices");
var sendDeviceCommand = /* @__PURE__ */ __name(async (command, deviceIds = [], params = {}) => {
  let unit = "can_all";
  let commandId;
  if (deviceIds.length === 1) {
    unit = "can_by_uid";
    commandId = deviceIds[0];
  } else if (deviceIds.length > 1) {
    unit = "can_selected";
    commandId = deviceIds;
  }
  const payload = __spreadValues(__spreadProps(__spreadValues({
    command
  }, params), {
    unit
  }), commandId !== void 0 && { commandId });
  const response = await fetch("/control.json", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!response.ok) {
    throw new Error("HTTP ".concat(response.status, " - Command failed"));
  }
}, "sendDeviceCommand");
var prepareUpdate = /* @__PURE__ */ __name(async (file, targetType = "can_by_uid", targetId = null) => {
  const fileBuffer = await file.arrayBuffer();
  const crc32Signed = CRC32.buf(new Uint8Array(fileBuffer));
  const crc32Unsigned = crc32Signed >>> 0;
  const payload = {
    command: "update_prepare",
    update_type: targetType,
    update_size: file.size.toString(),
    update_crc: crc32Unsigned.toString()
  };
  if (targetId) {
    payload.update_id = targetId;
  }
  const response = await fetch("/control.json", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!response.ok) throw new Error("Prepare update failed");
  return payload;
}, "prepareUpdate");
var completeUpdate = /* @__PURE__ */ __name(async (targetType = "can_by_uid", targetId = null) => {
  const payload = {
    command: "update_complete",
    update_type: targetType
  };
  if (targetId) {
    payload.update_id = targetId;
  }
  const response = await fetch("/control.json", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });
  if (!response.ok) throw new Error("Prepare update failed");
  return payload;
}, "completeUpdate");
var apiRequest = /* @__PURE__ */ __name(async (endpoint, method = "GET", data = null) => {
  try {
    const options = { method, headers: { "Content-Type": "application/json" } };
    if (data) options.body = JSON.stringify(data);
    const response = await fetch(endpoint, options);
    return await response.json();
  } catch (error) {
    console.error("API Error:", error);
    return null;
  }
}, "apiRequest");

// src/js/components/App.js
init_preact_shim();

// node_modules/preact/hooks/dist/hooks.module.js
init_preact_shim();
init_preact_module();
var t3;
var r2;
var u2;
var i2;
var o2 = 0;
var f2 = [];
var c2 = l;
var e2 = c2.__b;
var a2 = c2.__r;
var v2 = c2.diffed;
var l2 = c2.__c;
var m2 = c2.unmount;
var s2 = c2.__;
function p2(n3, t4) {
  c2.__h && c2.__h(r2, n3, o2 || t4), o2 = 0;
  var u3 = r2.__H || (r2.__H = { __: [], __h: [] });
  return n3 >= u3.__.length && u3.__.push({}), u3.__[n3];
}
__name(p2, "p");
function d2(n3) {
  return o2 = 1, h2(D2, n3);
}
__name(d2, "d");
function h2(n3, u3, i3) {
  var o3 = p2(t3++, 2);
  if (o3.t = n3, !o3.__c && (o3.__ = [i3 ? i3(u3) : D2(void 0, u3), function(n4) {
    var t4 = o3.__N ? o3.__N[0] : o3.__[0], r3 = o3.t(t4, n4);
    t4 !== r3 && (o3.__N = [r3, o3.__[1]], o3.__c.setState({}));
  }], o3.__c = r2, !r2.__f)) {
    var f3 = /* @__PURE__ */ __name(function(n4, t4, r3) {
      if (!o3.__c.__H) return true;
      var u4 = o3.__c.__H.__.filter(function(n5) {
        return !!n5.__c;
      });
      if (u4.every(function(n5) {
        return !n5.__N;
      })) return !c3 || c3.call(this, n4, t4, r3);
      var i4 = o3.__c.props !== n4;
      return u4.forEach(function(n5) {
        if (n5.__N) {
          var t5 = n5.__[0];
          n5.__ = n5.__N, n5.__N = void 0, t5 !== n5.__[0] && (i4 = true);
        }
      }), c3 && c3.call(this, n4, t4, r3) || i4;
    }, "f");
    r2.__f = true;
    var c3 = r2.shouldComponentUpdate, e3 = r2.componentWillUpdate;
    r2.componentWillUpdate = function(n4, t4, r3) {
      if (this.__e) {
        var u4 = c3;
        c3 = void 0, f3(n4, t4, r3), c3 = u4;
      }
      e3 && e3.call(this, n4, t4, r3);
    }, r2.shouldComponentUpdate = f3;
  }
  return o3.__N || o3.__;
}
__name(h2, "h");
function y2(n3, u3) {
  var i3 = p2(t3++, 3);
  !c2.__s && C2(i3.__H, u3) && (i3.__ = n3, i3.u = u3, r2.__H.__h.push(i3));
}
__name(y2, "y");
function A2(n3) {
  return o2 = 5, T2(function() {
    return { current: n3 };
  }, []);
}
__name(A2, "A");
function T2(n3, r3) {
  var u3 = p2(t3++, 7);
  return C2(u3.__H, r3) && (u3.__ = n3(), u3.__H = r3, u3.__h = n3), u3.__;
}
__name(T2, "T");
function x2(n3) {
  var u3 = r2.context[n3.__c], i3 = p2(t3++, 9);
  return i3.c = n3, u3 ? (null == i3.__ && (i3.__ = true, u3.sub(r2)), u3.props.value) : n3.__;
}
__name(x2, "x");
function j2() {
  for (var n3; n3 = f2.shift(); ) if (n3.__P && n3.__H) try {
    n3.__H.__h.forEach(z2), n3.__H.__h.forEach(B2), n3.__H.__h = [];
  } catch (t4) {
    n3.__H.__h = [], c2.__e(t4, n3.__v);
  }
}
__name(j2, "j");
c2.__b = function(n3) {
  r2 = null, e2 && e2(n3);
}, c2.__ = function(n3, t4) {
  n3 && t4.__k && t4.__k.__m && (n3.__m = t4.__k.__m), s2 && s2(n3, t4);
}, c2.__r = function(n3) {
  a2 && a2(n3), t3 = 0;
  var i3 = (r2 = n3.__c).__H;
  i3 && (u2 === r2 ? (i3.__h = [], r2.__h = [], i3.__.forEach(function(n4) {
    n4.__N && (n4.__ = n4.__N), n4.u = n4.__N = void 0;
  })) : (i3.__h.forEach(z2), i3.__h.forEach(B2), i3.__h = [], t3 = 0)), u2 = r2;
}, c2.diffed = function(n3) {
  v2 && v2(n3);
  var t4 = n3.__c;
  t4 && t4.__H && (t4.__H.__h.length && (1 !== f2.push(t4) && i2 === c2.requestAnimationFrame || ((i2 = c2.requestAnimationFrame) || w2)(j2)), t4.__H.__.forEach(function(n4) {
    n4.u && (n4.__H = n4.u), n4.u = void 0;
  })), u2 = r2 = null;
}, c2.__c = function(n3, t4) {
  t4.some(function(n4) {
    try {
      n4.__h.forEach(z2), n4.__h = n4.__h.filter(function(n5) {
        return !n5.__ || B2(n5);
      });
    } catch (r3) {
      t4.some(function(n5) {
        n5.__h && (n5.__h = []);
      }), t4 = [], c2.__e(r3, n4.__v);
    }
  }), l2 && l2(n3, t4);
}, c2.unmount = function(n3) {
  m2 && m2(n3);
  var t4, r3 = n3.__c;
  r3 && r3.__H && (r3.__H.__.forEach(function(n4) {
    try {
      z2(n4);
    } catch (n5) {
      t4 = n5;
    }
  }), r3.__H = void 0, t4 && c2.__e(t4, r3.__v));
};
var k2 = "function" == typeof requestAnimationFrame;
function w2(n3) {
  var t4, r3 = /* @__PURE__ */ __name(function() {
    clearTimeout(u3), k2 && cancelAnimationFrame(t4), setTimeout(n3);
  }, "r"), u3 = setTimeout(r3, 35);
  k2 && (t4 = requestAnimationFrame(r3));
}
__name(w2, "w");
function z2(n3) {
  var t4 = r2, u3 = n3.__c;
  "function" == typeof u3 && (n3.__c = void 0, u3()), r2 = t4;
}
__name(z2, "z");
function B2(n3) {
  var t4 = r2;
  n3.__c = n3.__(), r2 = t4;
}
__name(B2, "B");
function C2(n3, t4) {
  return !n3 || n3.length !== t4.length || t4.some(function(t5, r3) {
    return t5 !== n3[r3];
  });
}
__name(C2, "C");
function D2(n3, t4) {
  return "function" == typeof t4 ? t4(n3) : t4;
}
__name(D2, "D");

// src/js/components/StateTab.js
init_preact_shim();
init_preact_module();
var html = htm_module_default.bind(_);
var _a, _b, _c, _d, _e, _f, _g;
function StateTab() {
  const [changes, setChanges] = d2({});
  const [state, setState] = d2(null);
  const [file, setFile] = d2(null);
  const [progress, setProgress] = d2(null);
  const [updateType, setUpdateType] = d2("device");
  const handleUpload = /* @__PURE__ */ __name(async () => {
    if (!file) return;
    try {
      await prepareUpdate(file, "self");
      const formData = new FormData();
      formData.append("file", file);
      const xhr = new XMLHttpRequest();
      xhr.upload.onprogress = (e3) => {
        if (e3.lengthComputable) {
          setProgress(Math.round(e3.loaded / e3.total * 100));
        }
      };
      await new Promise((resolve, reject) => {
        xhr.onload = resolve;
        xhr.onerror = reject;
        xhr.open("POST", "/update/data", true);
        xhr.send(formData);
      });
      await completeUpdate("self");
      setProgress(100);
      alert("Update erfolgreich!");
    } catch (error) {
      console.error("Update failed:", error);
      alert("Update fehlgeschlagen: ".concat(error.message));
    } finally {
      setProgress(null);
    }
  }, "handleUpload");
  const [isDragOver, setIsDragOver] = d2(false);
  const handleDragOver = /* @__PURE__ */ __name((e3) => {
    e3.preventDefault();
    setIsDragOver(true);
  }, "handleDragOver");
  const handleDragLeave = /* @__PURE__ */ __name(() => {
    setIsDragOver(false);
  }, "handleDragLeave");
  const handleDrop = /* @__PURE__ */ __name((e3) => {
    e3.preventDefault();
    setIsDragOver(false);
    if (e3.dataTransfer.files && e3.dataTransfer.files[0]) {
      setFile(e3.dataTransfer.files[0]);
    }
  }, "handleDrop");
  y2(() => {
    loadState();
  }, []);
  const loadState = /* @__PURE__ */ __name(async () => {
    const data = await apiRequest("state.json");
    setState(data);
  }, "loadState");
  const handleChange = /* @__PURE__ */ __name((field, value, section = null) => {
    setChanges((prev) => {
      const newChanges = __spreadValues({}, prev);
      if (section) {
        newChanges[section] = __spreadProps(__spreadValues({}, newChanges[section]), { [field]: value });
      } else {
        newChanges[field] = value;
      }
      return newChanges;
    });
  }, "handleChange");
  const handleMqttLogging = /* @__PURE__ */ __name(async (value) => {
    await apiRequest("/control.json", "POST", {
      command: "mqtt_logging",
      enabled: value
    });
    loadState();
  }, "handleMqttLogging");
  const getCurrentValue = /* @__PURE__ */ __name((field, section) => {
    var _a14, _b7, _c5, _d2, _e2, _f2;
    return (_f2 = (_e2 = (_d2 = (_c5 = (_a14 = changes[section]) == null ? void 0 : _a14[field]) != null ? _c5 : (_b7 = state == null ? void 0 : state[section]) == null ? void 0 : _b7[field]) != null ? _d2 : changes[field]) != null ? _e2 : state == null ? void 0 : state[field]) != null ? _f2 : "";
  }, "getCurrentValue");
  const saveConfig = /* @__PURE__ */ __name(async () => {
    await apiRequest("/control.json", "POST", __spreadValues({
      command: "save_config"
    }, changes));
    loadState();
    setChanges({});
  }, "saveConfig");
  const refreshState = /* @__PURE__ */ __name(async () => {
    const data = await apiRequest("state.json");
    setState(data);
  }, "refreshState");
  const restartDevice = /* @__PURE__ */ __name(async () => {
    await apiRequest("/control.json", "POST", {
      command: "restart",
      unit: "self"
    });
  }, "restartDevice");
  return html(_g || (_g = __template(['\n    <div class="tab-content">\n\n      \n      <!-- General Section -->\n      <div class="detail-section">\n        <div class="device-config">\n          <div class="config-grid">\n            <div class="config-group">\n              <h5>General</h5>\n              ', '\n              <div class="config-row">\n                <label>Hostname</label>\n                <input \n                  class="config-input"\n                  type="text" \n                  value=', " \n                  onInput=", ' \n                />\n              </div>\n                \n              <div class="config-row">\n                <label>Username</label>\n                <input \n                  class="config-input"\n                  type="text" \n                  value=', " \n                  onInput=", '\n                />\n              </div>\n                \n              <div class="config-row">\n                <label>Password</label>\n                <input \n                  class="config-input"\n                  type="password" \n                  value=', " \n                  onInput=", '\n                />\n              </div>\n                \n              <div class="config-row">\n                <label>Update Delay</label>\n                <input \n                  class="config-input"\n                  type="text" \n                  value=', " \n                  onInput=", '\n                />\n              </div>\n            </div>\n      \n            <!-- WiFi Section -->\n            <div class="config-group">\n              <h5>WiFi</h5>\n              ', '\n              \n              <div class="config-row">\n                <label>Mode</label>\n                <select\n                  class="config-input"\n                  value=', " \n                  onChange=", '\n                >\n                  <option value="ap">Access Point</option>\n                  <option value="client">Client</option>\n                  <option value="off">Off</option>\n                </select>\n              </div>\n                \n              <div class="config-row">\n                <label>SSID</label>\n                <input \n                  class="config-input"\n                  type="text" \n                  value=', " \n                  onInput=", '\n                />\n              </div>\n                \n              <div class="config-row">\n                <label>Password</label>\n                <input \n                  class="config-input"\n                  type="text" \n                  value=', " \n                  onInput=", '\n                />\n              </div>\n            </div>\n      \n      <!-- MQTT Section -->\n            <div class="config-group">\n              <h5>MQTT</h5>\n              ', '\n              \n              <div class="config-row">\n                <label>Server URI</label>\n                <input\n                  class="config-input"\n                  type="text" \n                  value=', " \n                  onInput=", '\n                  placeholder="mqtt://IP:PORT"\n                />\n              </div>\n                \n              <div class="config-row">\n                <label>Username</label>\n                <input \n                  class="config-input"\n                  type="text" \n                  value=', " \n                  onInput=", '\n                />\n              </div>\n                \n              <div class="config-row">\n                <label>Password</label>\n                <input \n                  class="config-input"\n                  type="text" \n                  value=', " \n                  onInput=", '\n                />\n              </div>\n            </div>\n      \n      <!-- CAN Bus Section -->\n            <div class="config-group">\n              <h5>CAN Bus</h5>\n              ', '\n              \n              <div class="config-row">\n                <label>Baudrate</label>\n                <select \n                  class="config-input"\n                  value=', "\n                  onChange=", '\n                >\n                  <option value="b50">50 KBit/s</option>\n                  <option value="b22_222">22.222 KBit/s</option>\n                  <option value="b25">25 KBit/s</option>\n                  <option value="b100">100 KBit/s</option>\n                </select>\n              </div>\n                \n              <div class="config-row">\n                <label>\n                  <input \n                    type="checkbox" \n                    checked=', "\n                    onChange=", '\n                  />\n                  MQTT Logging\n                </label>\n              </div>\n            </div>\n\n            <div class="config-group">\n              <h5>Firmware Update</h5>\n              <div class="config-group-content">\n                <div class="upload-container">\n                  <label \n                      class=', "\n                      onDragOver=", "\n                      onDragLeave=", "\n                      onDrop=", '\n                  >\n                    <div class="file-upload-icon">\n                        <i class="fas fa-cloud-upload-alt"></i>\n                    </div>\n                    <div class="file-upload-text">\n                        ', '\n                    </div>\n                    <div class="file-upload-hint">\n                        or click to browse (.bin files only)\n                    </div>\n                    <input \n                        type="file" \n                        accept=".bin"\n                        onChange=', "\n                    />\n                  </label>\n                </div>\n                ", '\n              </div>\n              <div class="save-button">\n                <button\n                  class="primary"\n                  onClick=', "\n                  disabled=", '\n                >\n                    Start Update\n                </button>\n              </div>\n            </div>\n          </div>\n        </div>\n        <div class="save-button">\n          <button class="primary" onClick=', " disabled=", ">\n            Save\n          </button>\n          <button onClick=", ">Refresh</button>\n          <button onClick=", ">Restart</button>\n        </div>\n      </div>\n    </div>\n  "])), state && html(_a || (_a = __template(['\n                <div class="status-item"><span class="status-label">Firmware Version:</span> <span class="status-value">', '</span></div>\n                <div class="status-item"><span class="status-label">Uptime:</span> <span class="status-value">', '</span></div>\n                <div class="status-item"><span class="status-label">Hostname:</span> <span class="status-value">', "</span></div>\n              "])), state.firmware_version, state.uptime, state.hostname), getCurrentValue("hostname"), (e3) => handleChange("hostname", e3.target.value), getCurrentValue("username"), (e3) => handleChange("username", e3.target.value), getCurrentValue("password"), (e3) => handleChange("password", e3.target.value), getCurrentValue("update_delay") || 10, (e3) => handleChange("update_delay", e3.target.value), (state == null ? void 0 : state.wifi) && html(_b || (_b = __template(['\n                <div class="status-item"><span class="status-label">Connection State:</span> <span class="status-value">', '</span></div>\n                <div class="status-item"><span class="status-label">IPv4 Address:</span> <span class="status-value">', '</span></div>\n                <div class="status-item"><span class="status-label">IPv6 Address:</span> <span class="status-value">', '</span></div>\n                <div class="status-item"><span class="status-label">Gateway:</span> <span class="status-value">', '</span></div>\n                <div class="status-item"><span class="status-label">DNS Server:</span> <span class="status-value">', "</span></div>\n              "])), state.wifi.state, state.wifi.ipv4, state.wifi.ipv6, state.wifi.gateway, state.wifi.dns), getCurrentValue("mode", "wifi") || "ap", (e3) => handleChange("mode", e3.target.value, "wifi"), getCurrentValue("ssid", "wifi") || "", (e3) => handleChange("ssid", e3.target.value, "wifi"), getCurrentValue("password", "wifi") || "", (e3) => handleChange("password", e3.target.value, "wifi"), (state == null ? void 0 : state.mqtt) && html(_c || (_c = __template(['\n                <div class="status-item"><span class="status-label">Connection State:</span> <span class="status-value">', '</span></div>\n                <div class="status-item"><span class="status-label">Messages Received:</span> <span class="status-value">', '</span></div>\n                <div class="status-item"><span class="status-label">Messages Send:</span> <span class="status-value">', "</span></div>\n              "])), state.mqtt.state, state.mqtt.received, state.mqtt.sent), getCurrentValue("uri", "mqtt") || "", (e3) => handleChange("uri", e3.target.value, "mqtt"), getCurrentValue("username", "mqtt") || "", (e3) => handleChange("username", e3.target.value, "mqtt"), getCurrentValue("password", "mqtt") || "", (e3) => handleChange("password", e3.target.value, "mqtt"), (state == null ? void 0 : state.canbus) && html(_d || (_d = __template(['\n                <div class="status-item"><span class="status-label">Messages Received:</span> <span class="status-value">', '</span></div>\n                <div class="status-item"><span class="status-label">Messages Send:</span> <span class="status-value">', "</span></div>\n              "])), state.canbus.received, state.canbus.sent), getCurrentValue("baudrate", "canbus") || "b50", (e3) => handleChange("baudrate", e3.target.value, "canbus"), getCurrentValue("mqtt_logging", "canbus") || false, (e3) => handleMqttLogging(e3.target.checked), "file-upload ".concat(isDragOver ? "drag-over" : ""), handleDragOver, handleDragLeave, handleDrop, file ? file.name : "Drag & Drop firmware file here", (e3) => setFile(e3.target.files[0]), progress !== null && html(_f || (_f = __template(['\n                  <div class="progress-container">\n                    <div class="progress-bar" style=', '></div>\n                    <span class="progress-text">', "%</span>\n                  </div>\n                  ", "\n                "])), { width: "".concat(progress, "%") }, progress, progress === 100 && html(_e || (_e = __template(['<div class="success-message">Update completed successfully!</div>'])))), handleUpload, !file, saveConfig, !Object.keys(changes).length, refreshState, restartDevice);
}
__name(StateTab, "StateTab");

// src/js/components/DevicesTab.js
init_preact_shim();
init_preact_module();

// src/js/components/DeviceTable.js
init_preact_shim();

// src/js/components/DeviceRow.js
init_preact_shim();
init_preact_module();

// src/js/components/DeviceDetails.js
init_preact_shim();
init_preact_module();

// src/js/components/RelaisControl.js
init_preact_shim();
init_preact_module();

// src/js/utils.js
init_preact_shim();
var html2 = htm_module_default.bind(_);
var decimalToHex = /* @__PURE__ */ __name((d3, padding = 2) => {
  const hex = Number(d3).toString(16).toUpperCase();
  return hex.padStart(padding, "0");
}, "decimalToHex");
var _a2;
var formatCanId = /* @__PURE__ */ __name((uid, msg) => {
  return html2(_a2 || (_a2 = __template(['\n    <div class="can-id-preview">\n      <span style="color:#000000">', '</span>\n      <span style="color:#00AA00">', '</span>\n      <span style="color:#AA0000">', '</span>\n      <span style="color:#0000AA">', '</span>\n      <span style="color:#000000">|</span>\n    </div>\n  '])), uid.substring(2, 4), uid.substring(4, 6), uid.substring(6, 8), msg);
}, "formatCanId");

// src/js/components/RelaisControl.js
var html3 = htm_module_default.bind(_);
var _a3, _b2, _c2;
function RelaisControl({ deviceUid }) {
  const [type, setType] = d2("relais");
  const [bank, setBank] = d2(0);
  const [num, setNum] = d2(0);
  const [state, setState] = d2("off");
  const [time, setTime] = d2(0);
  const getHexCommand = /* @__PURE__ */ __name(() => {
    const typeHex = type === "rollershutter" ? "83" : "82";
    let stateHex = "00";
    if (state === "on") stateHex = "03";
    if (state === "up") stateHex = "01";
    if (state === "down") stateHex = "02";
    const timeHex = decimalToHex(time, 6).match(/.{2}/g).reverse().join("");
    return {
      type: typeHex,
      num: decimalToHex(num, 2),
      state: stateHex,
      time: timeHex,
      bank: decimalToHex(bank, 2)
    };
  }, "getHexCommand");
  const executeCommand = /* @__PURE__ */ __name(() => {
    const hex = getHexCommand();
    sendDeviceCommand(type, [deviceUid], {
      num: parseInt(num),
      state: parseInt(hex.state, 16),
      time: parseInt(time),
      bank: parseInt(bank)
    });
  }, "executeCommand");
  return html3(_c2 || (_c2 = __template(['\n    <div class="config-group-content">\n      <div class="control-row">\n        <label>Type:</label>\n        <select class="config-input" value=', " onChange=", '>\n          <option value="relais">Relais</option>\n          <option value="rollershutter">Rollershutter</option>\n        </select>\n      </div>\n\n      <div class="control-row">\n        <label>Bank:</label>\n        <input class="config-input" type="number" min="0" max="255" value=', " \n          onInput=", ' />\n      </div>\n\n      <div class="control-row">\n        <label>Number:</label>\n        <input class="config-input" type="number" min="0" max="255" value=', " \n          onInput=", ' />\n      </div>\n\n      <div class="control-row">\n        <label>State:</label>\n        <select class="config-input" value=', " onChange=", ">\n          ", '\n        </select>\n      </div>\n\n      <div class="control-row">\n        <label>Time (ms):</label>\n        <input class="config-input" type="number" min="0" value=', " \n          onInput=", ' />\n      </div>\n\n      <div class="can-preview">\n        ', '\n        <span style="color:#0000AA">', '</span>\n        <span style="color:#00AA00">', '</span>\n        <span style="color:#AA0000">', '</span>\n        <span style="color:#0000AA">', '</span>\n      </div>\n    </div>\n    <div class="save-button">\n      <button class="primary" onClick=', ">Execute</button>\n    </div>\n  "])), type, (e3) => setType(e3.target.value), bank, (e3) => setBank(e3.target.value), num, (e3) => setNum(e3.target.value), state, (e3) => setState(e3.target.value), type === "relais" ? html3(_a3 || (_a3 = __template(['\n            <option value="off">OFF</option>\n            <option value="on">ON</option>\n          ']))) : html3(_b2 || (_b2 = __template(['\n            <option value="off">OFF</option>\n            <option value="up">UP</option>\n            <option value="down">DOWN</option>\n          ']))), time, (e3) => setTime(e3.target.value), formatCanId(deviceUid, getHexCommand().type), getHexCommand().num, getHexCommand().state, getHexCommand().time, getHexCommand().bank, executeCommand);
}
__name(RelaisControl, "RelaisControl");

// src/js/components/PwmControl.js
init_preact_shim();
init_preact_module();
var html4 = htm_module_default.bind(_);
var _a4, _b3, _c3;
function PwmControl({ deviceUid }) {
  const [value, setValue] = d2(0);
  const [bank, setBank] = d2(0);
  const [bitmask, setBitmask] = d2(0);
  const [instant, setInstant] = d2(false);
  const [lastSelected, setLastSelected] = d2(null);
  const containerRef = A2();
  const toggleBit = /* @__PURE__ */ __name((bit, isShiftSelect = false, forceState = null) => {
    setBitmask((prev) => {
      let newMask = prev;
      if (isShiftSelect && lastSelected !== null) {
        const start = Math.min(lastSelected, bit);
        const end = Math.max(lastSelected, bit);
        const referenceState = (prev & 1 << 23 - lastSelected) !== 0;
        for (let i3 = start; i3 <= end; i3++) {
          if (referenceState) {
            newMask |= 1 << 23 - i3;
          } else {
            newMask &= ~(1 << 23 - i3);
          }
        }
      } else if (forceState !== null) {
        if (forceState) {
          newMask |= 1 << 23 - bit;
        } else {
          newMask &= ~(1 << 23 - bit);
        }
      } else {
        newMask = prev ^ 1 << 23 - bit;
      }
      return newMask;
    });
    setLastSelected(bit);
  }, "toggleBit");
  const handleMouseDown = /* @__PURE__ */ __name((bit, e3) => {
    if (e3.button !== 0) return;
    const initialBitState = (bitmask & 1 << 23 - bit) !== 0;
    if (e3.shiftKey && lastSelected !== null) {
      toggleBit(bit, true);
    } else {
      toggleBit(bit, false);
      const handleMouseMove = /* @__PURE__ */ __name((e4) => {
        const checkboxElements = containerRef.current.querySelectorAll(".bitmask-checkbox");
        const currentCheckbox = document.elementFromPoint(e4.clientX, e4.clientY);
        const index = Array.from(checkboxElements).indexOf(currentCheckbox);
        if (index >= 0 && index <= 23) {
          toggleBit(index, false, !initialBitState);
        }
      }, "handleMouseMove");
      const handleMouseUp = /* @__PURE__ */ __name(() => {
        window.removeEventListener("mousemove", handleMouseMove);
        window.removeEventListener("mouseup", handleMouseUp);
      }, "handleMouseUp");
      window.addEventListener("mousemove", handleMouseMove);
      window.addEventListener("mouseup", handleMouseUp);
    }
  }, "handleMouseDown");
  const handleClick = /* @__PURE__ */ __name((bit, e3) => {
    e3.preventDefault();
  }, "handleClick");
  const getHexCommand = /* @__PURE__ */ __name(() => {
    const bitmaskHex = decimalToHex(bitmask, 6).match(/.{2}/g).join("");
    return {
      cmd: "5A",
      value: decimalToHex(value, 2),
      bitmask: bitmaskHex,
      bank: decimalToHex(bank, 2)
    };
  }, "getHexCommand");
  const executeCommand = /* @__PURE__ */ __name(() => {
    const hex = getHexCommand();
    sendDeviceCommand("lamps", [deviceUid], {
      value: parseInt(value),
      bitmask: parseInt(bitmask),
      bank: parseInt(bank)
    });
  }, "executeCommand");
  return html4(_c3 || (_c3 = __template(['\n    <div class="config-group-content">\n      <div class="control-row">\n        <label>Value (0-255):</label>\n        <input type="range" min="0" max="255" value=', " \n          onInput=", " />\n        <span>", '</span>\n      </div>\n\n      <div class="control-row">\n        <label>Bank:</label>\n        <input type="number" min="0" max="255" value=', " \n          onInput=", ' />\n      </div>\n\n      <div class="bitmask-container" ref=', '>\n        <div class="bitmask-header">\n          ', '\n        </div>\n        <div class="bitmask-grid">\n          ', '\n        </div>\n      </div>\n\n      <div class="can-preview">\n        ', '\n        <span style="color:#0000AA">', '</span>\n        <span style="color:#00AA00">', '</span>\n        <span style="color:#0000AA">', '</span>\n      </div>\n    </div>\n    <div class="save-button">\n      <button class="primary" onClick=', ">Execute</button>\n    </div>\n  "])), value, (e3) => setValue(e3.target.value), value, bank, (e3) => setBank(e3.target.value), containerRef, Array.from({ length: 24 }).map((_2, i3) => html4(_a4 || (_a4 = __template(['\n            <span class="bitmask-number">', "</span>\n          "])), 23 - i3)), Array.from({ length: 24 }).map((_2, i3) => html4(_b3 || (_b3 = __template(['\n            <input\n              type="checkbox"\n              class="bitmask-checkbox"\n              checked=', "\n              onMouseDown=", "\n              onClick=", "\n            />\n          "])), (bitmask & 1 << 23 - i3) !== 0, (e3) => handleMouseDown(i3, e3), (e3) => handleClick(i3, e3))), formatCanId(deviceUid, getHexCommand().cmd), getHexCommand().value, getHexCommand().bitmask, getHexCommand().bank, executeCommand);
}
__name(PwmControl, "PwmControl");

// src/js/components/FirmwareUpload.js
init_preact_shim();
init_preact_module();

// src/js/stores/deviceStore.js
init_preact_shim();
init_preact_module();
var html5 = htm_module_default.bind(_);
var DeviceContext = K();
var _a5;
function DeviceProvider({ children }) {
  const [devices, setDevices] = d2([]);
  const [selected, setSelected] = d2([]);
  const refreshDevices = /* @__PURE__ */ __name(async () => {
    try {
      const response = await loadDevices();
      setDevices(response.devices || []);
    } catch (error) {
      console.error("Failed to load devices:", error);
      setDevices([]);
    }
  }, "refreshDevices");
  const value = {
    devices,
    selected,
    setSelected,
    refreshDevices,
    toggleDevice: /* @__PURE__ */ __name((uid) => {
      setSelected(
        (prev) => prev.includes(uid) ? prev.filter((id) => id !== uid) : [...prev, uid]
      );
    }, "toggleDevice"),
    toggleAll: /* @__PURE__ */ __name((uids) => {
      setSelected(
        (prev) => prev.length === uids.length ? [] : uids
      );
    }, "toggleAll")
  };
  y2(() => {
    refreshDevices();
  }, []);
  return html5(_a5 || (_a5 = __template(["\n    <", " value=", ">\n      ", "\n    <//>\n  "])), DeviceContext.Provider, value, children);
}
__name(DeviceProvider, "DeviceProvider");
var useDeviceStore = /* @__PURE__ */ __name(() => x2(DeviceContext), "useDeviceStore");

// src/js/components/FirmwareUpload.js
var html6 = htm_module_default.bind(_);
var _a6, _b4, _c4;
function FirmwareUpload({ deviceUid }) {
  const { refreshDevices } = useDeviceStore();
  const [file, setFile] = d2(null);
  const [progress, setProgress] = d2(null);
  const [updateType, setUpdateType] = d2("device");
  const handleUpload = /* @__PURE__ */ __name(async () => {
    if (!file) return;
    try {
      await prepareUpdate(file, "can_by_uid", deviceUid);
      const formData = new FormData();
      formData.append("file", file);
      const xhr = new XMLHttpRequest();
      xhr.upload.onprogress = (e3) => {
        if (e3.lengthComputable) {
          setProgress(Math.round(e3.loaded / e3.total * 100));
        }
      };
      await new Promise((resolve, reject) => {
        xhr.onload = resolve;
        xhr.onerror = reject;
        xhr.open("POST", "/update/data", true);
        xhr.send(formData);
      });
      await completeUpdate("can_by_uid", deviceUid);
      setProgress(100);
      refreshDevices();
      alert("Update erfolgreich!");
    } catch (error) {
      console.error("Update failed:", error);
      alert("Update fehlgeschlagen: ".concat(error.message));
    } finally {
      setProgress(null);
    }
  }, "handleUpload");
  const [isDragOver, setIsDragOver] = d2(false);
  const handleDragOver = /* @__PURE__ */ __name((e3) => {
    e3.preventDefault();
    setIsDragOver(true);
  }, "handleDragOver");
  const handleDragLeave = /* @__PURE__ */ __name(() => {
    setIsDragOver(false);
  }, "handleDragLeave");
  const handleDrop = /* @__PURE__ */ __name((e3) => {
    e3.preventDefault();
    setIsDragOver(false);
    if (e3.dataTransfer.files && e3.dataTransfer.files[0]) {
      setFile(e3.dataTransfer.files[0]);
    }
  }, "handleDrop");
  return html6(_c4 || (_c4 = __template(['\n    <div class="config-group-content">\n      <div class="upload-container">\n        <label \n            class=', "\n            onDragOver=", "\n            onDragLeave=", "\n            onDrop=", '\n        >\n          <div class="file-upload-icon">\n              <i class="fas fa-cloud-upload-alt"></i>\n          </div>\n          <div class="file-upload-text">\n              ', '\n          </div>\n          <div class="file-upload-hint">\n              or click to browse (.bin files only)\n          </div>\n          <input \n              type="file" \n              accept=".bin"\n              onChange=', "\n          />\n        </label>\n      </div>\n      ", '\n    </div>\n    <div class="save-button">\n      <button\n        class="primary"\n        onClick=', "\n        disabled=", "\n      >\n          Start Update\n      </button>\n    </div>\n  "])), "file-upload ".concat(isDragOver ? "drag-over" : ""), handleDragOver, handleDragLeave, handleDrop, file ? file.name : "Drag & Drop firmware file here", (e3) => setFile(e3.target.files[0]), progress !== null && html6(_b4 || (_b4 = __template(['\n        <div class="progress-container">\n          <div class="progress-bar" style=', '></div>\n          <span class="progress-text">', "%</span>\n        </div>\n        ", "\n      "])), { width: "".concat(progress, "%") }, progress, progress === 100 && html6(_a6 || (_a6 = __template(['<div class="success-message">Update completed successfully!</div>'])))), handleUpload, !file);
}
__name(FirmwareUpload, "FirmwareUpload");

// src/js/components/DeviceDetails.js
var html7 = htm_module_default.bind(_);
var _a7;
function DeviceDetails({ device }) {
  var _a14, _b7;
  const [config, setConfig] = d2({
    rollershutter_mode: device.rollershutter_mode,
    device_id: device.device_id,
    device_type: device.device_type,
    hwrev: device.hwrev,
    relais_mode: device.relais_mode,
    extension_mode: device.extension_mode,
    custom_string: device.custom_string,
    baudrate: device.baudrate,
    uid0: device.uid0,
    uid1: device.uid1
  });
  const { refreshDevices } = useDeviceStore();
  const handleCommand = /* @__PURE__ */ __name(async (command) => {
    try {
      await sendDeviceCommand(command, device.uid);
      refreshDevices();
    } catch (error) {
      alert("Error: ".concat(command, " failed - ").concat(error.message));
    }
  }, "handleCommand");
  const handleSaveConfig = /* @__PURE__ */ __name(async (uid, config2) => {
    try {
      const diff = Object.fromEntries(
        Object.entries(config2).filter(([key, value]) => device[key] !== value)
      );
      if (Object.keys(diff).length === 0) {
        alert("Keine \xC4nderungen zu speichern.");
        return;
      }
      await sendDeviceCommand("save", [uid], diff);
      refreshDevices();
    } catch (error) {
      alert("Save failed: " + error.message);
    }
  }, "handleSaveConfig");
  const handleChange = /* @__PURE__ */ __name((field, value) => {
    setConfig((prev) => __spreadProps(__spreadValues({}, prev), {
      [field]: field === "hwrev" ? parseInt(value) || 0 : value
    }));
  }, "handleChange");
  return html7(_a7 || (_a7 = __template(['\n    <div class="device-details">\n      <div class="detail-section">\n        <div class="device-config">\n          <div class="config-grid">\n            <div class="config-group">\n              <h5>Status</h5>\n              <div class="status-item"><span class="status-label">Firmware Version:</span> <span class="status-value">', '</span></div>\n              <div class="status-item"><span class="status-label">HW Revision:</span> <span class="status-value">', '</span></div>\n              <div class="status-item"><span class="status-label">Device UID0:</span> <span class="status-value">', '</span></div>\n              <div class="status-item"><span class="status-label">Device UID1:</span> <span class="status-value">', '</span></div>\n              <div class="status-item"><span class="status-label">Baudrate:</span> <span class="status-value">', '</span></div>\n              <div class="status-item"><span class="status-label">Uptime:</span> <span class="status-value">', '</span></div>\n            </div>\n    \n            <div class="config-group">\n              <h5>Basic Settings</h5>\n              <div class="config-row">\n                <label>Device ID</label>\n                <input type="text" value=', " \n                  onChange=", '\n                  class="config-input small" />\n              </div>\n              \n              <div class="config-row">\n                <label>Device Type</label>\n                <input type="text" value=', " \n                  onChange=", '\n                  class="config-input small" />\n              </div>\n              \n              <div class="config-row">\n                <label>HW Rev</label>\n                <input type="text" value=', " \n                  onChange=", '\n                  class="config-input small" />\n              </div>\n              \n              <div class="config-row">\n                <label>Custom String</label>\n                <input type="text" value=', " \n                  onChange=", '\n                  maxlength="8" class="config-input" />\n              </div>\n      \n              <div class="config-row">\n                <label>Baudrate</label>\n                <select\n                  value=', "\n                  onChange=", '\n                  class="config-input"\n                >\n                  <option value="b50">50 KBit/s</option>\n                  <option value="b22_222">22.222 KBit/s</option>\n                  <option value="b25">25 KBit/s</option>\n                  <option value="b100">100 KBit/s</option>\n                </select>\n              </div>\n            </div>\n      \n            <div class="config-group">\n              <h5>Device Behavior</h5>\n              <div class="config-row">\n                <label>Relais Mode</label>\n                <select \n                  value=', " \n                  onInput=", '\n                  class="config-input"\n                >\n                  <option value="OFF">Off</option>\n                  <option value="RELAIS">Relais</option>\n                  <option value="SWROLLERSHUTTER">Software Rollershutter</option>\n                  <option value="HWROLLERSHUTTER">Hardware Rollershutter</option>\n                </select>\n              </div>\n      \n              <div class="config-row">\n                <label>Extension Mode</label>\n                <select \n                  value=', " \n                  onInput=", '\n                  class="config-input"\n                >\n                  <option value="OFF">Off</option>\n                  <option value="BUTTONS">Buttons</option>\n                  <option value="RELAIS">Relais</option>\n                  <option value="SWROLLERSHUTTER">Software Rollershutter</option>\n                  <option value="HWROLLERSHUTTER">Hardware Rollershutter</option>\n                  <option value="PWM">PWM</option>\n                  <option value="SENSORS">Sensors</option>\n                  <option value="LEGACY_SENSORS">Legacy Sensors</option>\n                </select>\n              </div>\n            </div>\n          </div>\n          <div class="save-button">\n            <button class="primary" onClick=', ">Save All Changes</button>\n            <button onClick=", ">Refresh</button>\n            <button onClick=", ">Ping</button>\n            <button onClick=", ">Restart</button>\n            <button onClick=", ">Silence On</button>\n            <button onClick=", '>Silence Off</button>\n          </div>\n        </div>\n      </div>\n      <div class="detail-section">\n        <div class="device-config">\n          <div class="config-grid">\n            <div class="config-group">\n              <h5>Firmware Update</h5>\n              <', " deviceUid=", ' />\n            </div>\n            <div class="config-group">\n              <h5>Relais Control</h5>\n              <', " deviceUid=", ' />\n            </div>\n            <div class="config-group">\n              <h5>PWM Control</h5>\n              <', " deviceUid=", " />\n            </div>\n          </div>\n        </div>\n      </div>\n\n    </div>\n  "])), device.version, device.hwrev, device.uid0, device.uid1, device.baudrate, device.uptime, config.device_id, (e3) => handleChange("device_id", e3.target.value), config.device_type, (e3) => handleChange("device_type", e3.target.value), config.hwrev, (e3) => handleChange("hwrev", e3.target.value), config.custom_string, (e3) => handleChange("custom_string", e3.target.value), config.baudrate, (e3) => handleChange("baudrate", e3.target.value), (_a14 = config.relais_mode) != null ? _a14 : "OFF", (e3) => handleChange("relais_mode", e3.target.value), (_b7 = config.extension_mode) != null ? _b7 : "OFF", (e3) => handleChange("extension_mode", e3.target.value), () => handleSaveConfig(device.uid, config), () => handleCommand("refresh"), () => handleCommand("ping"), () => handleCommand("restart"), () => handleCommand("silence_on"), () => handleCommand("silence_off"), FirmwareUpload, device.uid, RelaisControl, device.uid, PwmControl, device.uid);
}
__name(DeviceDetails, "DeviceDetails");

// src/js/components/DeviceRow.js
var html8 = htm_module_default.bind(_);
var _a8, _b5;
function DeviceRow({ device, selected, onToggle, rowIndex }) {
  const [expanded, setExpanded] = d2(false);
  return html8(_b5 || (_b5 = __template(["\n    <tr id=", ' class="base-row ', '" onClick=', '>\n      <td><input type="checkbox" checked=', " onClick=", " /></td>\n      <td>", "</td>\n      <td>", "</td>\n      <td>", "</td>\n      <td>", "</td>\n      <td>", "</td>\n      <td>", "</td>\n      <td>", "</td>\n      <td>", "</td>\n    </tr>\n    \n    ", "\n  "])), device.uid, rowIndex % 2 === 0 ? "even" : "odd", () => setExpanded(!expanded), selected, (e3) => {
    e3.stopPropagation();
    onToggle(device.uid);
  }, device.uid, device.device_id, device.device_type, device.device_type_name, device.custom_string, device.last_seen, device.state, device.last_error, expanded && html8(_a8 || (_a8 = __template(['\n      <tr class="details ', '">\n        <td colspan="9">\n          <', " device=", " />\n        </td>\n      </tr>\n    "])), rowIndex % 2 === 0 ? "even" : "odd", DeviceDetails, device));
}
__name(DeviceRow, "DeviceRow");

// src/js/components/DeviceTable.js
var html9 = htm_module_default.bind(_);
var _a9, _b6;
function DeviceTable() {
  const { devices, selected, toggleDevice, toggleAll, refreshDevices } = useDeviceStore();
  return html9(_b6 || (_b6 = __template(['\n    <table id="device_table">\n      <thead>\n        <tr class="header">\n          <th><input type="checkbox" onChange=', " checked=", ' /></th>\n          <th>Unique ID</th>\n          <th>Device ID</th>\n          <th>Type</th>\n          <th>Type Name</th>\n          <th>Custom String</th>\n          <th>Last Seen</th>\n          <th>State</th>\n          <th>Error</th>\n        </tr>\n      </thead>\n      <tbody id="device_table_body">\n        ', "\n      </tbody>\n    </table>\n  "])), () => toggleAll(devices.map((d3) => d3.uid)), selected.length === devices.length, devices.map((device, index) => html9(_a9 || (_a9 = __template(["\n          <", " \n            device=", "\n            selected=", "\n            onToggle=", "\n            rowIndex=", "\n          />\n        "])), DeviceRow, device, selected.includes(device.uid), () => toggleDevice(device.uid), index)));
}
__name(DeviceTable, "DeviceTable");

// src/js/components/BatchControls.js
init_preact_shim();
init_preact_module();
var html10 = htm_module_default.bind(_);
var _a10;
function BatchControls() {
  const { devices, selected, refreshDevices } = useDeviceStore();
  const [pingUid, setPingUid] = d2("");
  const handleCommand = /* @__PURE__ */ __name(async (command) => {
    try {
      await sendDeviceCommand(command, selected);
      refreshDevices();
    } catch (error) {
      alert("Error: ".concat(error.message));
    }
  }, "handleCommand");
  const handlePingUid = /* @__PURE__ */ __name(() => {
    if (!pingUid.trim()) {
      alert("Bitte UID eingeben!");
      return;
    }
    console.log("pingUid", [pingUid.trim()]);
    sendDeviceCommand("ping", [pingUid.trim()]);
  }, "handlePingUid");
  return html10(_a10 || (_a10 = __template(['\n    <div class="batch-controls">\n      <button onClick=', ">Refresh All</button>\n      <button onClick=", ">\n        ", "\n      </button>\n      <button onClick=", ">Silence On</button>\n      <button onClick=", ">Silence Off</button>\n      <button onClick=", ">\n        ", '\n      </button>\n      <input\n        type="text"\n        value=', "\n        onInput=", '\n        placeholder="Device UID eingeben"\n        class="config-input"\n      />\n      <button onClick=', ">Ping UID</button>\n      <button onClick=", ">Scan</button>\n    </div>\n  "])), refreshDevices, () => handleCommand("ping"), selected.length ? "Ping Selected" : "Broadcast Ping", () => handleCommand("silence_on"), () => handleCommand("silence_off"), () => handleCommand("restart"), selected.length ? "Restart Selected" : "Restart All", pingUid, (e3) => setPingUid(e3.target.value), handlePingUid, () => handleCommand("scan"));
}
__name(BatchControls, "BatchControls");

// src/js/components/DevicesTab.js
var html11 = htm_module_default.bind(_);
var _a11;
function DevicesTab() {
  const { devices, selected, refreshDevices } = useDeviceStore();
  return html11(_a11 || (_a11 = __template(['\n    <div class="tab-content">\n      <', " />\n      <", " />\n    </div>\n  "])), BatchControls, DeviceTable);
}
__name(DevicesTab, "DevicesTab");

// src/js/components/App.js
var html12 = htm_module_default.bind(_);
var _a12;
function App() {
  const [activeTab, setActiveTab] = d2("state");
  return html12(_a12 || (_a12 = __template(['\n    <div class="app">\n      <', ">\n        <", " />\n        <", " />\n      <//>\n    </div>\n  "])), DeviceProvider, StateTab, DevicesTab);
}
__name(App, "App");

// src/js/app.js
var html13 = htm_module_default.bind(_);
var _a13;
E(html13(_a13 || (_a13 = __template(["<", " />"])), App), document.getElementById("app"));
