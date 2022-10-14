function update_click(uid) {
    console.log("update " + uid);
    var update_file = document.getElementById(uid + "_file_upload").files[0];
    var req = new XMLHttpRequest();
    var formData = new FormData();

    if (uid == "by_type")
    {
        var type = document.getElementById("selected_device_type").value;
        console.log(type);
        formData.append("type:"+type, update_file);
        req.open("POST", '/update/can');
    }
    else if (uid == "selected")
    {
        // not implemented yet
        //req.open("POST", '/update/'+uid);
    }
    else if (uid == "device_update")
    {
        var xmlhttp = new XMLHttpRequest();   // new HttpRequest instance 
        xmlhttp.open("POST", "/update/device/state");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({ "prepare": true }));
        
        formData.append("device_update", update_file);
        req.open("POST", '/update/device/data', false);
    }
    else
    {
        formData.append(uid, update_file);
        req.open("POST", '/update/can');
    }
    req.send(formData);
    
    if (uid == "device_update")
    {
        var xmlhttp = new XMLHttpRequest();   // new HttpRequest instance 
        xmlhttp.open("POST", "/update/device/state", false);
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({ "complete": true }));
    }
}
