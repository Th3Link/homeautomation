use <schalter_berker.scad>;

/*
difference() {
blocks(8,32);
translate([plate_h/2-hole_dx/2,plate_h/2-hole_dy/2,0]) holes(h=hole_h+plate_t);
}
*/
intersection() {
cube([20,40,2]);
blocks_with_holes(1);
    
}
//blocks(8,32,2);