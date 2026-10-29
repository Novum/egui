"""Regenerate vector.ttf with fonttools. Original geometric test outlines, no external fonts."""

from pathlib import Path
from copy import deepcopy
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.pens.t2CharStringPen import T2CharStringPen
from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools.ttLib.tables.otTables import PaintFormat
from fontTools.ttLib.tables.TupleVariation import TupleVariation
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString


def glyph(contours):
    pen = TTGlyphPen(None)
    for contour in contours:
        pen.moveTo(contour[0])
        for point in contour[1:]:
            if len(point) == 4:
                pen.qCurveTo(point[:2], point[2:])
            else:
                pen.lineTo(point)
        pen.closePath()
    return pen.glyph()


box = [(100, 0), (100, 800), (900, 800), (900, 0)]
hole = [(300, 200), (700, 200), (700, 600), (300, 600)]
upper = [(100, 400), (100, 800), (900, 800), (900, 400)]
quad = [(100, 0), (100, 800, 500, 800), (900, 800, 900, 0)]
outlines = {
    ".notdef": glyph([box, hole]),
    "space": glyph([]),
    "box": glyph([box]),
    "hole": glyph([box, hole]),
    "quad": glyph([quad]),
    "upper": glyph([upper]),
    "color": glyph([]),
    "foreground": glyph([]),
}
fb = FontBuilder(1000, isTTF=True)
fb.setupGlyphOrder(list(outlines))
fb.setupCharacterMap({32: "space", 65: "box", 79: "hole", 81: "quad", 70: "foreground", 0x1F600: "color"})
fb.setupGlyf(outlines)
fb.setupHorizontalMetrics({name: (1000, 100) for name in outlines})
fb.setupHorizontalHeader(ascent=800, descent=-200)
fb.setupNameTable({"familyName": "Vector test", "styleName": "Regular", "uniqueFontIdentifier": "egui-vector-test", "fullName": "Vector test", "psName": "VectorTest"})
fb.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
fb.setupPost()
fb.setupMaxp()
fb.font["COLR"] = buildCOLR({"color": [("box", 0), ("upper", 1)], "foreground": [("hole", 0xFFFF)]}, version=0)
fb.font["CPAL"] = buildCPAL([[(1, 0, 0, 1), (0, 0, 1, 0.5)]])
fb.font["head"].created = fb.font["head"].modified = 3400000000
fb.font.recalcTimestamp = False
fb.save(Path(__file__).with_name("vector.ttf"))

# A weight axis changes the geometry, so the curve cache must distinguish it.
variable = FontBuilder(font=deepcopy(fb.font))
variable.setupFvar([("wght", 100, 400, 900, "Weight")], [])
variations = {name: [] for name in outlines}
variations["box"] = [TupleVariation({"wght": (0, 1, 1)},
    [(0, 0), (0, 0), (200, 0), (200, 0)] + [(0, 0)] * 4)]
variable.setupGvar(variations)
variable.save(Path(__file__).with_name("vector-variable.ttf"))

# The same outlines with a translucent, transformed COLRv1 foreground layer.
fb.font["COLR"] = buildCOLR({
    "foreground": {
        "Format": PaintFormat.PaintTranslate,
        "dx": 50, "dy": 0,
        "Paint": {
            "Format": PaintFormat.PaintGlyph, "Glyph": "hole",
            "Paint": {"Format": PaintFormat.PaintSolid, "PaletteIndex": 0xFFFF, "Alpha": 0.5},
        },
    },
}, version=1)
fb.save(Path(__file__).with_name("vector-colrv1.ttf"))

# COLRv1 gradients and nested transformed clips.
line = {"Extend": 0, "ColorStop": [
    {"StopOffset": 0, "PaletteIndex": 0, "Alpha": 1},
    {"StopOffset": 1, "PaletteIndex": 1, "Alpha": 1},
]}
paints = {}
for name, paint in [
    ("box", {"Format": PaintFormat.PaintLinearGradient, "ColorLine": line,
        "x0": 100, "y0": 0, "x1": 900, "y1": 0, "x2": 100, "y2": 800}),
    ("quad", {"Format": PaintFormat.PaintRadialGradient, "ColorLine": line,
        "x0": 500, "y0": 400, "r0": 0, "x1": 500, "y1": 400, "r1": 500}),
    ("hole", {"Format": PaintFormat.PaintSweepGradient, "ColorLine": line,
        "centerX": 500, "centerY": 400, "startAngle": 0, "endAngle": 360}),
]:
    paints[name] = {"Format": PaintFormat.PaintGlyph, "Glyph": "box", "Paint": paint}
paints["foreground"] = {
    "Format": PaintFormat.PaintRotateAroundCenter,
    "angle": 30, "centerX": 500, "centerY": 400,
    "Paint": {"Format": PaintFormat.PaintGlyph, "Glyph": "box",
        "Paint": {"Format": PaintFormat.PaintRotateAroundCenter,
            "angle": -30, "centerX": 500, "centerY": 400,
            "Paint": {"Format": PaintFormat.PaintGlyph, "Glyph": "quad",
                "Paint": {"Format": PaintFormat.PaintSolid, "PaletteIndex": 0, "Alpha": 1}}}},
}
fb.font["COLR"] = buildCOLR(paints, version=1,
    clipBoxes={name: (200, 100, 800, 700) for name in paints})
fb.save(Path(__file__).with_name("vector-paints.ttf"))

# Porter-Duff groups, including an isolated nested group.
def solid(glyph_name, palette, alpha):
    return {"Format": PaintFormat.PaintGlyph, "Glyph": glyph_name,
        "Paint": {"Format": PaintFormat.PaintSolid, "PaletteIndex": palette, "Alpha": alpha}}

def composite(source, backdrop, mode):
    return {"Format": PaintFormat.PaintComposite, "SourcePaint": source,
        "BackdropPaint": backdrop, "CompositeMode": mode}

source = solid("quad", 0, 0.75)
backdrop = solid("box", 1, 0.5)
fb.font["COLR"] = buildCOLR({
    "box": composite(source, backdrop, 5),
    "hole": composite(source, backdrop, 7),
    "quad": composite(source, backdrop, 9),
    "foreground": composite(composite(source, backdrop, 5), solid("hole", 0, 0.5), 3),
}, version=1)
fb.save(Path(__file__).with_name("vector-composite.ttf"))

# One glyph per W3C blend mode (Screen through Luminosity), mapped from "a".
# Crossed gradients cover both branches of the piecewise modes, and a
# translucent source checks the unpremultiplied blend.
modes = range(13, 28)
blend_outlines = {".notdef": glyph([box]), "space": glyph([]), "box": glyph([box])}
blend_outlines.update({f"blend{mode}": glyph([]) for mode in modes})
fb = FontBuilder(1000, isTTF=True)
fb.setupGlyphOrder(list(blend_outlines))
fb.setupCharacterMap({32: "space", **{97 + i: f"blend{mode}" for i, mode in enumerate(modes)}})
fb.setupGlyf(blend_outlines)
fb.setupHorizontalMetrics({name: (1000, 100) for name in blend_outlines})
fb.setupHorizontalHeader(ascent=800, descent=-200)
fb.setupNameTable({"familyName": "Vector blend test", "styleName": "Regular", "uniqueFontIdentifier": "egui-vector-blend-test", "fullName": "Vector blend test", "psName": "VectorBlendTest"})
fb.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
fb.setupPost()
fb.setupMaxp()

def gradient(first, second, alpha, x1, y1):
    return {"Format": PaintFormat.PaintGlyph, "Glyph": "box", "Paint": {
        "Format": PaintFormat.PaintLinearGradient,
        "ColorLine": {"Extend": 0, "ColorStop": [
            {"StopOffset": 0, "PaletteIndex": first, "Alpha": alpha},
            {"StopOffset": 1, "PaletteIndex": second, "Alpha": alpha},
        ]},
        "x0": 100, "y0": 0, "x1": x1, "y1": y1, "x2": 100 + y1, "y2": x1 - 100}}

source = gradient(0, 1, 0.8, 900, 0)
backdrop = gradient(2, 3, 1, 100, 800)
fb.font["COLR"] = buildCOLR({f"blend{mode}": composite(source, backdrop, mode) for mode in modes}, version=1)
fb.font["CPAL"] = buildCPAL([[(0.05, 0.1, 0.9, 1), (1, 0.85, 0.2, 1), (0.9, 0.2, 0.3, 1), (0.2, 0.95, 0.6, 1)]])
fb.font["head"].created = fb.font["head"].modified = 3400000000
fb.font.recalcTimestamp = False
fb.save(Path(__file__).with_name("vector-blend.ttf"))

# A CFF cubic outline checks the cubic-to-quadratic path independently of glyf.
pen = T2CharStringPen(1000, None)
pen.moveTo((100, 0))
pen.curveTo((100, 1100), (900, 1100), (900, 0))
pen.closePath()
cubic = pen.getCharString()
fb = FontBuilder(1000, isTTF=False)
fb.setupGlyphOrder([".notdef", "cubic"])
fb.setupCharacterMap({67: "cubic"})
fb.setupCFF("VectorCubicTest", {"FullName": "Vector cubic test", "FamilyName": "Vector cubic test", "Weight": "Regular"}, {".notdef": cubic, "cubic": cubic}, {})
fb.setupHorizontalMetrics({name: (1000, 100) for name in [".notdef", "cubic"]})
fb.setupHorizontalHeader(ascent=900, descent=-100)
fb.setupNameTable({"familyName": "Vector cubic test", "styleName": "Regular", "uniqueFontIdentifier": "egui-vector-cubic-test", "fullName": "Vector cubic test", "psName": "VectorCubicTest"})
fb.setupOS2(sTypoAscender=900, sTypoDescender=-100, usWinAscent=900, usWinDescent=100)
fb.setupPost()
fb.font["head"].created = fb.font["head"].modified = 3400000000
fb.font.recalcTimestamp = False
fb.save(Path(__file__).with_name("vector-cff.otf"))

# Deterministic GSUB/GPOS coverage, independent of installed system fonts.
shaping = {
    ".notdef": glyph([box, hole]), "space": glyph([]),
    "A": glyph([box]), "V": glyph([quad]),
    "f": glyph([[(0, 0), (0, 800), (400, 800), (400, 0)]]),
    "i": glyph([[(0, 0), (0, 600), (200, 600), (200, 0)]]),
    "f_i": glyph([[(0, 0), (0, 800), (600, 800), (600, 0)]]),
    "f.alt": glyph([quad]), "a": glyph([box, hole]),
    "acute": glyph([[(0, 0), (100, 200), (200, 200), (100, 0)]]),
}
fb = FontBuilder(1000, isTTF=True)
fb.setupGlyphOrder(list(shaping))
fb.setupCharacterMap({32: "space", 65: "A", 86: "V", 102: "f", 105: "i", 97: "a", 0x301: "acute"})
fb.setupGlyf(shaping)
fb.setupHorizontalMetrics({name: (dict(f=500, i=300, f_i=700, acute=0).get(name, 1000), 0) for name in shaping})
fb.setupHorizontalHeader(ascent=1000, descent=-200)
fb.setupNameTable({"familyName": "Vector shaping test", "styleName": "Regular", "uniqueFontIdentifier": "egui-vector-shaping-test", "fullName": "Vector shaping test", "psName": "VectorShapingTest"})
fb.setupOS2(sTypoAscender=1000, sTypoDescender=-200, usWinAscent=1000, usWinDescent=200)
fb.setupPost()
fb.setupMaxp()
addOpenTypeFeaturesFromString(fb.font, """
languagesystem DFLT dflt;
languagesystem latn dflt;
feature kern { pos A V -150; } kern;
feature liga { sub f i by f_i; } liga;
feature calt { sub f' f by f.alt; } calt;
markClass acute <anchor 0 0> @TOP;
feature mark { pos base a <anchor 500 800> mark @TOP; } mark;
""")
fb.font["head"].created = fb.font["head"].modified = 3400000000
fb.font.recalcTimestamp = False
fb.save(Path(__file__).with_name("vector-shaping.ttf"))
