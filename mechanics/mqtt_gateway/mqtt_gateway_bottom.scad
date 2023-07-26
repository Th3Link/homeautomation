$fn=20;

use <../library/din_clip_01.scad>

pcb = [75+1,51+1,2];
holes_d = [67,43];
h0 = [(pcb[0]-holes_d[0])/2,(pcb[1]-holes_d[1])/2];
clip_w = 15;
holes = [
    h0,
    [h0[0]+holes_d[0],h0[1]+holes_d[1]],
    [h0[0]+holes_d[0],h0[1]],
    [h0[0],h0[1]+holes_d[1]],
];


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

translate([(15/2)-t1,0,0])rotate([0,90,0]) din_clip(clip_w);

//hanging clib
translate([pcb[0]-2*clip_w,0,0]) difference() {
translate([clip_w,0,0]) rotate([0,90,0]) din_clip(clip_w*2);
translate([0,0,0]) rotate([0,40,0]) translate([0,0,-clip_w*3]) cube([clip_w*3,pcb[1],clip_w*3]);
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
    translate([8,-t1,0]) cube([18, 11+t1,6]);
    
    //rj45 can
    translate([25,-t1,0]) cube([17, 18+t1,6]);

    //rj45 lan
    translate([46,-t1,0]) cube([18, 23+t1,6]);
    
    //pins
    //translate([8,45,0]) cube([18.5,3.5,6]);
    
    //component hight
    translate([0,0,0]) cube([pcb[0], pcb[1],5]);
        }
}
for ( hole = holes ){
difference() {
translate([hole[0], hole[1],0]) cylinder(r=7/2,h=6);
    translate([hole[0], hole[1],0]) cylinder(r=bore_r+0.6,h=6);
}
}
}
}
lid();
//bottom();