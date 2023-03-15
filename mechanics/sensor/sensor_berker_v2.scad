$fn=80;
outer = 55.5;
radius = 1;
t=3;
spacer=9;

dht22=1;
pir=0;
nightlight=1;
vents=0;

module vents(l=20,r=1,t=3,d=1.5) {
    iterations=3;
    translate([-(iterations*(2*r)+(iterations-1)*d)/2,0,0])
    for ( i = [0 : iterations-1] ) {
        translate([r+(2*r+d)*i,-(l/2-r),0]) hull() {
        cylinder(r=1,h=3);
        translate([0,l-2*r,0]) cylinder(r=1,h=3);
        }
    }
}

module base(outer,t,radius) {
    hull() {
        translate([-outer/2+1,-outer/2+1,0]) cylinder(r=radius, h=t);
        translate([-outer/2+1,outer/2-1,0]) cylinder(r=radius, h=t);
        translate([outer/2-1,outer/2-1,0]) cylinder(r=radius, h=t);
        translate([outer/2-1,-outer/2+1,0]) cylinder(r=radius, h=t);
    }
}

module pcb_mount_holes(dist_x,dist_y,r,h) {
    translate([dist_x/2,0,0])
    {
        translate([0,dist_y/2,0]) cylinder(r=r,h=h);
        translate([0,-dist_y/2,0]) cylinder(r=r,h=h);
    }
    translate([-dist_x/2,0,0])
    {
        translate([0,dist_y/2,0]) cylinder(r=r,h=h);
        translate([0,-dist_y/2,0]) cylinder(r=r,h=h);
    }
}

module pcb_mount_spacer_side(dist_y,r,h)
{
        translate([0,-dist_y/2,0]) cylinder(r=r,h=h);
        translate([0,dist_y/2,0]) cylinder(r=r,h=h);
}

module pcb_mount_spacer(dist_x,dist_y,r,h,t) {
            hull() {
                translate([-dist_x/2,0,0]) pcb_mount_spacer_side(dist_y=dist_y,r=r,h=h);
            }
            hull() {
                translate([dist_x/2,0,0]) pcb_mount_spacer_side(dist_y=dist_y,r=r,h=h);
            }
            translate([-dist_x/2,0,h]) pcb_mount_spacer_side(dist_y=dist_y,r=r,h=t);
            translate([dist_x/2,0,h]) pcb_mount_spacer_side(dist_y=dist_y,r=r,h=t);
}

module side_clip_hook(clamp_edge, length=20)
{
    translate([0,0,-0.8]) difference() {
       union () {
    fill_cube_side= sqrt(clamp_edge*clamp_edge*2)/2;
    translate([0.3,0,fill_cube_side]) {
        rotate([0,45,0]) translate([-clamp_edge/2,-length/2,-clamp_edge/2]) cube([clamp_edge,length,clamp_edge]);
        //translate([0,-length/2,0]) cube([fill_cube_side,length,fill_cube_side]);
    }
    translate([-1.4,(-length)/2,0]) cube([3,length,2]);
    }
    translate([1,-length/2,0]) cube([10,length,10]);
    translate([-3,-length/2,3]) cube([10,length,10]);
}
}

module side_clips(clamp_edge=3,length=20) {
    translate([-49.3/2,0,t]) {
        side_clip_hook(3, length);
    }
    translate([49.3/2,0,t]) {
        mirror([1,0,0]) side_clip_hook(3, length);
    }
}

module nightlight_square(d=20) {
    base(d,3,1);
}

difference() {
    //round corner cube (plate)
    union() {
        base(outer,t,radius);
        pcb_mount_spacer(dist_x=43,dist_y=28,r=5/2,h=spacer,t=t);
        rotate([0,0,90]) {
        translate([0,-3-16,0]) side_clips(length=6);
        translate([0,+3+16,0]) side_clips(length=6);
        }
    }
    translate([0,0,0.5]) { 
        translate([-25,3.5+16,0]) translate([-2,-3.5,0]) cube([4,7,t]);
        translate([-25,-(3.5+16),0]) translate([-2,-3.5,0]) cube([4,7,t]);
        translate([25,3.5+16,0]) translate([-2,-3.5,0]) cube([4,7,t]);
        translate([25,-(3.5+16),0]) translate([-2,-3.5,0]) cube([4,7,t]);
    }
    if (pir==1)
    {
    // bore for the pir
    cylinder(r=21/2,h=t);
    }
    
    if (dht22==1)
    {
    // hole for dht22
    translate([-10,21/2,0.8]) cube([20.5,15.5,t]);
    translate([-10+4.5,21/2+1,0]) cube([20.5-5.5,15.5-2,0.8]);
    }
    
    if (nightlight==1)
    {
        translate([0,0,0.5]) nightlight_square();
    }
    if (vents==1)
    {
        //holes for ventilation
        translate([14.5+0.7,0,0]) vents(t=t,d=1,r=0.9);
        translate([-14.5-0.7,0,0]) vents(t=t,d=1,r=0.9);
    }
    // holes in the plate for the pcb mount (for longer screws since i dont have shorter ones here)
    translate([0,0,0.5]) pcb_mount_holes(dist_x=43,dist_y=28,r=1.8/2,h=spacer+t);
}