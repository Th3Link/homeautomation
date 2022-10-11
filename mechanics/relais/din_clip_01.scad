CLIP_H = 10;
HOLE_DEPTH = 9;
HOLE_DIAMETER = 2.9;

module din_clip(h=CLIP_H) {
		linear_extrude(height=h, center=true, convexity=5) {
			import(file="din_clip_01.dxf", layer="0", $fn=64);
		}

}

din_clip();

