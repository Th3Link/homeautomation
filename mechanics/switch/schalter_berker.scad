$fn=50;
hole_dx = 32;
hole_dy = 14.8;
hole_r = 2.98;
hole_h = 10;

plate_h = 71;
plate_h2 = 71;
plate_r = 90/2;
plate_holes_r = 30;
plate_holes_rk = 3.4/2;
plate_holes_rg = 5.5/2;
plate_holes_len = 15;
plate_t = 1.7;

switch_x = 5;
switch_y = 5;
switch_z = 2;
switch_dx = 32;
switch_dy = 25;

pcd_mount_r = 1.8/2;
pcb_mount_dx = 28;
pcb_mount_dy = 43;

module slottedCylinder(r,h,rot=0)
{
    cylinder(r=r,h=h);
    rotate([0,0,rot]) translate([-r*2+1,0,h/2]) cube([r*4,r/2,h], center=true);
}

module holes(h) {
    slottedCylinder(r=hole_r,h=h,rot=180);
    translate([hole_dx,0,0]) slottedCylinder(r=hole_r,h=h);
    translate([0,hole_dy,0]) slottedCylinder(r=hole_r,h=h,rot=180);
    translate([hole_dx,hole_dy,0]) slottedCylinder(r=hole_r,h=h);
    
}

module mountRing() {
    scale([1,1,0.5]) minkowski() {
        intersection() {
            difference() {
                cylinder(r=plate_holes_r+0.01,h=plate_t);
                cylinder(r=plate_holes_r-0.01,h=plate_t);
            }
            translate([0,-plate_holes_r,plate_t/2]) cube([plate_holes_len,10,plate_t], center=true);
        }
        cylinder(r=plate_holes_rk,h=plate_t);
    }
    rotate([0,0,-13]) translate([0,-plate_holes_r]) cylinder(r=plate_holes_rg,h=plate_t);
}

module roundovercube(v,r, center=false) {
    
    translate([0,0,-v[2]*0.25]) scale([1,1,0.5]) minkowski() {
        cube([v[0],v[1],v[2]],center=center);
        cylinder(r=r,h=v[2]);
    }
}

module blocks(b,h,height=hole_h+plate_t) {
    translate([plate_h/2-15.5,plate_h/2,height/2]) roundovercube([b,h,height], 2, center=true);
    translate([plate_h/2+15.5,plate_h/2,height/2]) roundovercube([b,h,height], 2, center=true);
}

module switches() {
    translate([-switch_dx/2-switch_x/2,-switch_dy/2-switch_y/2,0]) union() {
        cube([switch_x,switch_y,switch_z]);
        translate([0,switch_dy,0]) cube([switch_x,switch_y,switch_z]);
        translate([switch_dx,0,0]) cube([switch_x,switch_y,switch_z]);
        translate([switch_dx,switch_dy,0]) cube([switch_x,switch_y,switch_z]);
    }
}

module pcb_mount(r,h)
{
    translate([-pcb_mount_dx/2,-pcb_mount_dy/2,0]) union() {
        cylinder(r=r,h=h);
        translate([0,pcb_mount_dy,0]) cylinder(r=r,h=h);
        translate([pcb_mount_dx,0,0]) cylinder(r=r,h=h);
        translate([pcb_mount_dx,pcb_mount_dy,0]) cylinder(r=r,h=h);
    }
}

module hcube(v,h1, h2)
{
    cube(v, center=true);
    translate([-v[0]/2+v[1]/2,0,0.75]) cube([v[1],h1,v[2]*1.5],center=true);
    translate([+v[0]/2-v[1]/2,0,0.75]) cube([v[1],h2,v[2]*1.5],center=true);
}

module plate() {
    intersection() {
        translate([0,(plate_h-plate_h2)/2,0]) cube([plate_h,plate_h2,plate_t]);
        translate([plate_h/2,plate_h/2,0]) cylinder(r=plate_r,h=plate_t);
    }
}

module blocks_with_holes(height) {
    translate([-14,-17.5,0]) difference() {
    blocks(8,32, height);
    translate([plate_h/2-hole_dx/2,plate_h/2-hole_dy/2,0]) holes(h=height);
    }
}

difference() {
union() {
    difference() {
    blocks(8,32);
    translate([plate_h/2-hole_dx/2,plate_h/2-hole_dy/2,0]) holes(h=hole_h+plate_t);
    }
    difference() {
        union() {
            plate();
            translate([plate_h/2,plate_h/2,0]) pcb_mount(3.5,4);
        }
        translate([plate_h/2,plate_h/2,0]) union() {
            mountRing();
            rotate([0,0,90]) mountRing();
            rotate([0,0,180]) mountRing();
            rotate([0,0,270])mountRing();
        }
        blocks(14,34);
    }
    translate([plate_h/2,plate_h/2,1.5]) hcube([plate_holes_r*2-6,2,3], 8,8);
    translate([plate_h/2,plate_h/2,1.5]) cube([10,10,3], center=true);
    translate([plate_h/2,plate_h/2,1.5]) rotate([0,0,90]) hcube([55,8,3],8,8);
}
//translate([plate_h/2,plate_h/2,0]) switches();
translate([plate_h/2,plate_h/2,0]) pcb_mount(pcd_mount_r,10);
}

difference() {
translate([plate_h/2,plate_h/2,plate_t]) rotate([0,0,90]) union() {
translate([-52/2,-20/2,0]) cube([3,20,1]);
translate([52/2-3,-20/2,0]) cube([3,20,1]);
// clamps for the frame
translate([-51.6/2+1.5,-20/2,2]) rotate([-90,0,0]) cylinder(r=4/2,h=20);
translate([51.6/2-3+1.5,-20/2,2]) rotate([-90,0,0]) cylinder(r=4/2,h=20);
}
}