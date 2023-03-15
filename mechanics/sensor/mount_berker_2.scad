$fn=40;
outer_x = 70;
outer_y = 72;
inner_x = 53;
inner_y = 55;
radius = 1;
t=2;
spacer=9;

module base(outer_x,outer_y,t,radius) {
    hull() {
        translate([-outer_x/2+1,-outer_y/2+1,0]) cylinder(r=radius, h=t);
        translate([-outer_x/2+1,outer_y/2-1,0]) cylinder(r=radius, h=t);
        translate([outer_x/2-1,outer_y/2-1,0]) cylinder(r=radius, h=t);
        translate([outer_x/2-1,-outer_y/2+1,0]) cylinder(r=radius, h=t);
    }
}

module side_clip_hook(clamp_edge, length=20)
{
    difference() {
       union () {
    fill_cube_side= sqrt(clamp_edge*clamp_edge*2)/2;
    translate([0,0,fill_cube_side]) {
        rotate([0,45,0]) translate([-clamp_edge/2,-length/2,-clamp_edge/2]) cube([clamp_edge,length,clamp_edge]);
        translate([0,-length/2,0]) cube([fill_cube_side,length,fill_cube_side]);
    }
    translate([-1.4,(-length)/2,0]) cube([3,length,2]);
    }
    translate([0,-length/2,0]) cube([10,length,10]);
    translate([-4,-length/2,3]) cube([10,length,10]);
}
}

module side_clips(clamp_edge=3,length=20) {
    translate([-49.0/2,0,t]) {
        side_clip_hook(3, length);
    }
    translate([49.0/2,0,t]) {
        mirror([1,0,0]) side_clip_hook(3, length);
    }
}

module mounting_slot(r2,r,h,w) {
    hull() {
    translate([0,w/2-r,0]) cylinder(r=r,h=h);
    translate([0,-w/2+r,0]) cylinder(r=r,h=h);
    }
}

difference() {
    //round corner cube (plate)
    union() {
        difference() {
            base(outer_x,outer_y,t,radius);
            base(inner_x,inner_y,t,radius);
            translate([30,0,0]) mounting_slot(5/2,3/2,t,10);
            translate([-30,0,0])mounting_slot(5/2,3/2,t,10);
        }
        translate([-inner_x/2-1.0,3+16.5,0]) translate([-3,-3,0]) cube([6,6,t]);
        translate([-inner_x/2-1.0,-(3+16.5),0]) translate([-3,-3,0]) cube([6,6,t]);
        translate([inner_x/2+1.0,3+16.5,0]) translate([-3,-3,0]) cube([6,6,t]);
        translate([inner_x/2+1.0,-(3+16.5),0]) translate([-3,-3,0]) cube([6,6,t]);
        translate([0,-3-16.5,0]) side_clips(length=6);
        translate([0,+3+16.5,0]) side_clips(length=6);
        
    }
}