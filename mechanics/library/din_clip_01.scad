CLIP_H = 10;
HOLE_DEPTH = 9;
HOLE_DIAMETER = 2.9;

module din_clip(h=CLIP_H, grow=1) {
        intersection() {
        translate([0,0,-h/2]) cube([20,20,h]);
		linear_extrude(height=h, center=true, convexity=5) {
			import(file="din_clip_01.dxf", layer="0", $fn=64);
		}
    }
        translate([0,grow,0]) intersection() {
        translate([0,20-grow,-h/2]) cube([20,40,h]);
		linear_extrude(height=h, center=true, convexity=5) {
			import(file="din_clip_01.dxf", layer="0", $fn=64);
		}
    }
}

din_clip();

