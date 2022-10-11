$fn=60;
outer = 56;
radius = 1;
t=3;
spacer=9;

difference() {
    //round corner cube (plate)
    hull() {
        translate([1,1,0]) cylinder(r=radius, h=t);
        translate([1,outer-1,0]) cylinder(r=radius, h=t);
        translate([outer-1,1,0]) cylinder(r=radius, h=t);
        translate([outer-1,outer-2,0]) cylinder(r=radius, h=t);
    }
    // bore for the pir
    translate([outer/2,outer/2,0]) cylinder(r=21/2,h=t);
    // hole for dht22
    translate([outer/2-10,outer/2+21/2,0]) cube([20.5,15.5,t]);
    // holes in the plate for the pcb mount (for longer screws since i dont have shorter ones here)
    translate([(outer-43)/2, (outer-28)/2, 1]) {
        cylinder(r=1.8/2,h=spacer+t);
        translate([0,28,0]) cylinder(r=1.8/2,h=spacer+t);
        translate([43,0,0]) cylinder(r=1.8/2,h=spacer+t);
        translate([43,28,0]) cylinder(r=1.8/2,h=spacer+t);
    }
}

//spacer
difference() {
    union() {
        translate([(outer-43)/2, (outer-28)/2, 0]) {
            hull() {
                cylinder(r=5/2,h=spacer);
                translate([0,28,0]) cylinder(r=5/2,h=spacer);
            }
            hull() {
                translate([43,0,0]) cylinder(r=5/2,h=spacer);
                translate([43,28,0]) cylinder(r=5/2,h=spacer);
            }
        }
        translate([(outer-43)/2, (outer-28)/2, 0]) {
            cylinder(r=5/2,h=spacer+t);
            translate([0,28,0]) cylinder(r=5/2,h=spacer+t);
            translate([43,0,0]) cylinder(r=5/2,h=spacer+t);
            translate([43,28,0]) cylinder(r=5/2,h=spacer+t);
        }
    }
    translate([(outer-43)/2, (outer-28)/2, 0]) {
        cylinder(r=1.8/2,h=spacer+t);
        translate([0,28,0]) cylinder(r=1.8/2,h=spacer+t);
        translate([43,0,0]) cylinder(r=1.8/2,h=spacer+t);
        translate([43,28,0]) cylinder(r=1.8/2,h=spacer+t);
    }
}

//side clips
translate([(outer-52)/2,(outer-20)/2,t]) cube([3,20,t]);
translate([(outer-52)/2+52-3,(outer-20)/2,t]) cube([3,20,t]);

translate([(outer-52)/2+1.5,(outer-20)/2,t+2]) rotate([-90,0,0]) cylinder(r=4/2,h=20);
translate([(outer-52)/2+52-3+1.5,(outer-20)/2,t+2]) rotate([-90,0,0]) cylinder(r=4/2,h=20);

// dht22 front cover
difference() {
translate([outer/2-10,outer/2+21/2,0]) cube([20.5,15.5,0.8]);
translate([outer/2-10+4.5,outer/2+21/2+1,0]) cube([20.5-5.5,15.5-2,0.8]);
}

//dht22 mount
translate([outer/2+20.5/2-2+4.5,outer/2+21/2+15.5/2,t]) difference() {
    hull() {
    translate([0,1.5,0]) cylinder(r=4.5/2,h=4.2);
    translate([0,-1.5,0])cylinder(r=4.5/2,h=4.2);
    translate([+1,1.5,0]) cylinder(r=4.5/2,h=4.2);
    translate([+1,-1.5,0])cylinder(r=4.5/2,h=4.2);
    }
    cylinder(r=2.7/2,h=4.2);
}