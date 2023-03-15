$fn=90;
hole_r = 2.2/2;
tube_r = 5/2;
hole_dx = 28;
h1 = 4.4;
h2 = 2;

module spacer() {
difference() {
    union() {
        cylinder(r=tube_r,h=h1);
        translate([hole_dx,0,0]) cylinder(r=tube_r,h=h1);
        hull() {
        cylinder(r=tube_r,h=h2);
        translate([hole_dx,0,0]) cylinder(r=tube_r,h=h2);
        }
    }
    cylinder(r=hole_r,h=h1);
    translate([hole_dx,0,0]) cylinder(r=hole_r,h=h1);
}
}

for (i = [0 : 0]) {
    translate([0,(5+1)*i,0]) spacer(); 
}