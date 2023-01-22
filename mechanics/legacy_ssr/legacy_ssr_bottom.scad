$fn=100;

use <../library/din_clip_01.scad>

holes = [
    [3.5,4],
    [3.5+42,4],
    [3.5+42+52,4],
    [3.5,4+42],
    [3.5+42+52,4+60]
];
pcb = [101,71,2];

t1 = 3;
t2 = 2*t1;
bore_r = 2.8/2;

module bottom() {
intersection() {
difference() {
union() {
difference() {
    translate([-t1,-t1,-t1+1]) cube([pcb[0]+2*t1, pcb[1]+2*t1,t2+1]);
    cube([pcb[0]+t1, pcb[1],5]);
}

for ( hole = holes ){
    hull() {
        translate([hole[0], hole[1]+3,0]) cylinder(r2=2*bore_r,r1=4*bore_r,h=5-2);
        translate([hole[0], hole[1]-3,0]) cylinder(r2=2*bore_r,r1=4*bore_r,h=5-2);
    }
}
}
for ( hole = holes ){
        translate([hole[0], hole[1],-2]) cylinder(r=bore_r,h=6);
    }
}
translate([-t1,-t1,-t1+1]) cube([pcb[0]+t1, pcb[1]+2*t1,t2+1]);
}

translate([(15/2)-t1,0,0])rotate([0,90,0]) din_clip(15);
translate([-t1,0,0]) difference() {
translate([-15+t1+pcb[0],0,0]) rotate([0,90,0]) din_clip(30);
translate([75,0,0]) rotate([0,40,0]) translate([0,0,-20-1]) cube([40,pcb[1],20]);
}
}

module lid() {
translate([0,0,30]) {
    union() {
    difference() {
        union() {
translate([-t1,-t1,0]) cube([pcb[0]+t1, pcb[1]+2*t1,6]);
translate([pcb[0],-t1,-9]) cube([2, pcb[1]+2*t1,15]);
        }
for ( hole = holes ){
        translate([hole[0], hole[1],0]) cylinder(r=bore_r+0.6,h=6);
    }
    //connectors
    translate([8,-t1,0]) cube([33, 14+t1,6]);
    translate([pcb[0]-44-7,-t1,0]) cube([44, 14+t1,6]);
    //fuse
    translate([pcb[0]-10,16,0]) cube([10, 27,6]);
    //leds
    translate([10,43.5,0]) cube([29, 5,6]);
    translate([51,43.5,0]) cube([31, 5,6]);
    //rj45
    translate([-t1,pcb[1]-15,0]) cube([19+t1, 15+t1,6]);

    //ds18b20
    translate([69-5,66-3.5,0]) cube([10,7.5,6]);
    //component hight
    translate([1,14,0]) cube([40, 22,5]);
    translate([47,14,0]) cube([40, 22,5]);
    translate([1,14+22,0]) cube([40, pcb[1]-14-22-1,4]);
    translate([47,14+22,0]) cube([40, pcb[1]-14-22-1,4]);
    translate([1,pcb[1]-15-1,0]) cube([80, 15 ,4]);
        }
}
difference() {
translate([holes[3][0], holes[3][1],0]) cylinder(r=2.5*bore_r,h=6);
    translate([holes[3][0], holes[3][1],0]) cylinder(r=bore_r+0.6,h=6);
}
}
}

bottom();