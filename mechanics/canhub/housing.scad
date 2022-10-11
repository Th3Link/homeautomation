$fn=90;
pcb_x = 44;
pcb_y = 133;
offset_y = 1;
hole_x = 35;
hole_y = 118;
hole_offset = 1.5;
clip_y = 22.5;
clip_x = 10.2;
clip_h = 4;
hole_r = 2.8/2;
t_bottom = 3;
t_side = 1.6;
module holes()
{
    cylinder(r=hole_r,h=2*t_bottom);
    translate([hole_x,0,0]) cylinder(r=hole_r,h=2*t_bottom);
    translate([0,hole_y,0]) cylinder(r=hole_r,h=2*t_bottom);
    translate([hole_x,hole_y,0]) cylinder(r=hole_r,h=2*t_bottom);
}

module clipmount_holes(add=0)
{
    translate([clip_x/2,clip_y/2,0]) cylinder(r=hole_r+add,h=clip_h+2*t_bottom);
    translate([pcb_x-clip_x/2,clip_y/2,0]) cylinder(r=hole_r+add,h=clip_h+2*t_bottom);
}

module clipmount()
{
    difference() {
        cube([pcb_x,clip_y,clip_h]);
        clipmount_holes(0.2);
    }
}

module housing()
{
    difference() {
        translate([-t_side,-t_side,0]) cube([pcb_x+2*t_side,pcb_y+2*t_side,11]);
        cube([pcb_x,pcb_y,11]);
    }
    cube([pcb_x,pcb_y,t_bottom]);
    translate([0,0,t_bottom]) difference() {
        cube([pcb_x,pcb_y,t_bottom]);
        translate([(pcb_x-30)/2,0,0]) cube([30,pcb_y-7,t_bottom]);
    }
}

difference()
{
    housing();
    translate([(pcb_x-hole_x)/2,(pcb_y-hole_y)/2+hole_offset,0]) holes();
    translate([0,(pcb_y-clip_y)/2,0]) clipmount_holes();
}