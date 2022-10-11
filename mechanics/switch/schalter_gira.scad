$fn=50;
hole_r = 7.3/2;
hole_h = 8.5;
hole_dx = 15;
hole_dy = 10+2*hole_r;

plate_h = 71;
plate_r = 78/2;
plate_holes_r = 30;
plate_holes_rk = 3.4/2;
plate_holes_rg = 5.7/2;
plate_holes_len = 15;
plate_t = 1.8;

switch_x = 5;
switch_y = 5;
switch_z = 2;
switch_dx = 32;
switch_dy = 25;

pcd_mount_r = 3.5/2;
pcb_mount_dx = 0;
pcb_mount_dy = 0;

module slottedCylinder(r,h,rot=0)
{
    cylinder(r=r,h=h);
    rotate([0,0,rot]) translate([-r*2+1,0,h/2]) cube([r*4,r/2,h], center=true);
}

module roundovercube(v,r, center=false) {
    c = 0;
    if (center)
    {
        c = 1;
    }
    translate([0,0,-v[2]*0.25]) scale([1,1,0.5]) minkowski() {
        cube([v[0],v[1],v[2]],center=center);
        translate([0,0,(-v[2]/2)*c]) cylinder(r=r,h=v[2]);
    }
}

module holes(h) {
    slottedCylinder(r=hole_r,h=h,rot=90);
    translate([hole_dx,0,0]) slottedCylinder(r=hole_r,h=h,rot=90);
    translate([0,hole_dy,0]) slottedCylinder(r=hole_r,h=h,rot=270);
    translate([hole_dx,hole_dy,0]) slottedCylinder(r=hole_r,h=h,rot=270);
    
}

module plate(r=15) {
   translate([r,r,plate_t/4]) roundovercube([plate_h-2*r,plate_h-2*r,plate_t],r);
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

module blocks(b,h) {
    translate([plate_h/2-11,plate_h/2,hole_h/2+plate_t/2]) 
    roundovercube([b,h,hole_h+plate_t], 2, center=true);
    translate([plate_h/2+11,plate_h/2,hole_h/2+plate_t/2]) 
    roundovercube([b,h,hole_h+plate_t], 2, center=true);
}

module blockextensions(b,h) {
    leverShift = 1.7;
    holderShift = 1.5;
    lever_y = 3;
    lever_x = 6;
    translate([plate_h/2-10,plate_h/2,hole_h/2+plate_t/2]) 
    union() {
        translate([-leverShift,h/2-lever_y/2,0]) roundovercube([lever_x+b,lever_y,hole_h+plate_t], 2, center=true);
        translate([holderShift,h/2-h/3/2,0]) roundovercube([b,h/3,hole_h+plate_t], 2, center=true);
        translate([-leverShift,-h/2+lever_y/2,0]) roundovercube([lever_x+b,lever_y,hole_h+plate_t], 2, center=true);
        translate([holderShift,-h/2+h/6,0]) roundovercube([b,h/3,hole_h+plate_t], 2, center=true);
    }
    translate([plate_h/2+10,plate_h/2,hole_h/2+plate_t/2]) 
    union() {
        translate([leverShift,h/2-lever_y/2,0]) roundovercube([lever_x+b,lever_y,hole_h+plate_t], 2, center=true);
        translate([-holderShift,h/2-h/6,0]) roundovercube([b,h/3,hole_h+plate_t], 2, center=true);
        translate([leverShift,-h/2+lever_y/2,0]) roundovercube([lever_x+b,lever_y,hole_h+plate_t], 2, center=true);
        translate([-holderShift,-h/2+h/6,0]) roundovercube([b,h/3,hole_h+plate_t], 2, center=true);
    }
}
module switches() {
    translate([-switch_dx/2-switch_x/2,-switch_dy/2-switch_y/2,0]) union() {
        cube([switch_x,switch_y,switch_z]);
        translate([0,switch_dy,0]) cube([switch_x,switch_y,switch_z]);
        translate([switch_dx,0,0]) cube([switch_x,switch_y,switch_z]);
        translate([switch_dx,switch_dy,0]) cube([switch_x,switch_y,switch_z]);
    }
}

module pcb_mount()
{
    translate([-pcb_mount_dx/2,-pcb_mount_dy/2,0]) union() {
        cylinder(r=pcd_mount_r,h=20);
        translate([0,pcb_mount_dy,0]) cylinder(r=pcd_mount_r,h=20);
    }
}

module hcube(v,l, h1, h2, h3)
{
    cube(v, center=true);
    translate([-v[0]/2+l/2,0,0.75]) cube([l,h1,h3],center=true);
    translate([+v[0]/2-l/2,0,0.75]) cube([l,h2,h3],center=true);
}


difference() {
union() {
    difference() {
    union() {
        blocks(4,27);
        blockextensions(9,27);
    }
    translate([plate_h/2-hole_dx/2,plate_h/2-hole_dy/2,0]) holes(h=hole_h+plate_t);
    }
    difference() {
        plate();
        translate([plate_h/2,plate_h/2,0]) union() {
        mountRing();
        rotate([0,0,90]) mountRing();
        rotate([0,0,180]) mountRing();
        rotate([0,0,270])mountRing();
        }
        blocks(19,29);
    }
    translate([plate_h/2,plate_h/2,1.5]) hcube([plate_holes_r*2-8.7,2,3], 3.15, 24,24, 3);
    //translate([plate_h/2,plate_h/2,1.5]) cube([10,10,3], center=true);
    translate([plate_h/2,plate_h/2,2.5]) rotate([0,0,90]) hcube([51.6,2,5],4,24, 24,3.5);
}
//translate([plate_h/2,plate_h/2,0]) switches();
//translate([plate_h/2,plate_h/2,0]) pcb_mount();
}
