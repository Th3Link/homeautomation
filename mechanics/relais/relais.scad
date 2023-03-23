use <../library/din_clip_01.scad>

PCB_HEIGHT=98;
PCB_WIDTH1=90;
PCB_WIDTH2=96;
PCB_T =2;
PCB_FREE_SPACE=34;
T=2;

module half(width) {
difference() {
    cube([width+1.5,PCB_HEIGHT+3,3+3*T]);
    translate([1.5,1.5,T]) union() { 
        translate([2,1.5,0]) cube([width-2, PCB_HEIGHT-3, 3]);
        translate([0,0,3])cube([width, PCB_HEIGHT, T]);
        translate([2,1.5,3+T]) cube([width-2, PCB_HEIGHT-3, T]);
    }
    translate([PCB_FREE_SPACE,PCB_HEIGHT,0]) cube([width-PCB_FREE_SPACE+1.5,3,3+3*T]);
}
}

half(PCB_WIDTH1);
translate([0,25,22]) half(PCB_WIDTH1);
//translate([PCB_WIDTH1+PCB_WIDTH2+20,0,0]) mirror([1,0,0]) half(PCB_WIDTH2);

difference() {
cube([T,PCB_HEIGHT+25+3,22+3+3*T]);
color("red") translate([0,0,3+3*T]) rotate([-48.5,0,0]) translate([0,-20,0]) cube([2,20,50]);
color("red") translate([0,PCB_WIDTH2+16.24,-14.97]) rotate([-48.5,0,0]) translate([0,-20,0]) cube([2,20,50]);
}

translate([25/2,25,T]) rotate([0,90,0]) din_clip(h=25);
