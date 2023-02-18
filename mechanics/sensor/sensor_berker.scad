$fn=60;
outer = 56;
radius = 1;
t=3;
spacer=9;
veml=1;
bme680=1;
bme280=0;
dht22=1;
pir=1;

module ventholes(h)
{
    for(i= [0 : 1 : 2]) {
            translate([3*i,0,0]) hull() {
            cylinder(r=1,h=h);
            translate([0,21,0]) cylinder(r=1,h=h);
            }
        }
}

difference() {
    //round corner cube (plate)
    hull() {
        translate([1,1,0]) cylinder(r=radius, h=t);
        translate([1,outer-1,0]) cylinder(r=radius, h=t);
        translate([outer-1,1,0]) cylinder(r=radius, h=t);
        translate([outer-1,outer-2,0]) cylinder(r=radius, h=t);
    }
    // bore for the pir
    if (pir) translate([outer/2,outer/2,0]) cylinder(r=21/2,h=t);
    // hole for dht22
    if (dht22) translate([outer/2-10,outer/2+21/2,0]) cube([20.5,15.5,t]);
    //holes for BME680
    if (bme680) translate([9,(outer-23)/2,0]) ventholes(t);
    //holes for BME280
    if (bme280) translate([22+(outer-22)/2,41,0]) rotate([0,0,90]) ventholes(t);
    //thin out for veml7700
    if (veml) translate([41,(outer-14)/2,1]) hull() {
        cylinder(r=1,h=3);
        translate([0,14,0]) cylinder(r=1,h=3);
        translate([5,0,0]) cylinder(r=1,h=3);
        translate([5,14,0]) cylinder(r=1,h=3);
    }

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
     //holes for BME680
    if (bme680) translate([9,(outer-23)/2,0])  ventholes(20);
}

//side clips
translate([(outer-52)/2,(outer-20)/2,t]) cube([3,20,t]);
translate([(outer-52)/2+52-3,(outer-20)/2,t]) cube([3,20,t]);

translate([(outer-52)/2+1.5,(outer-20)/2,t+2]) rotate([-90,0,0]) cylinder(r=4/2,h=20);
translate([(outer-52)/2+52-3+1.5,(outer-20)/2,t+2]) rotate([-90,0,0]) cylinder(r=4/2,h=20);

// dht22 front cover
if (dht22) difference() {
translate([outer/2-10,outer/2+21/2,0]) cube([20.5,15.5,0.8]);
translate([outer/2-10+4.5,outer/2+21/2+1,0]) cube([20.5-5.5,15.5-2,0.8]);
}

//dht22 mount
if (dht22) translate([outer/2+20.5/2-2+4.5,outer/2+21/2+15.5/2,t]) difference() {
    hull() {
    translate([0,1.5,0]) cylinder(r=4.5/2,h=4.2);
    translate([0,-1.5,0])cylinder(r=4.5/2,h=4.2);
    translate([+1,1.5,0]) cylinder(r=4.5/2,h=4.2);
    translate([+1,-1.5,0])cylinder(r=4.5/2,h=4.2);
    }
    cylinder(r=2.7/2,h=4.2);
}