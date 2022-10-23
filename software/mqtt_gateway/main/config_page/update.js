function update_click(uid) {
    console.log("update " + uid);
    var update_file = document.getElementById(uid + "_file_upload").files[0];
    var update_file_size = document.getElementById(uid + "_file_upload").files[0].size;
    var req = new XMLHttpRequest();
    var formData = new FormData();

    if (uid == "by_type")
    {
        var type = document.getElementById("selected_device_type").value;
        console.log(type);
        
        var xmlhttp = new XMLHttpRequest();
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({command:"update_prepare","update_type":"can_by_type","update_id":type,"update_size":update_file_size}));
        
        formData.append("type:"+type, update_file);
        req.open("POST", '/update/data');
    }
    else if (uid == "selected")
    {
        // not implemented yet
        //req.open("POST", '/update/'+uid);
    }
    else if (uid == "device_update")
    {
        var xmlhttp = new XMLHttpRequest();
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({command:"update_prepare","update_type":"self","update_size":update_file_size}));
        
        formData.append("device_update", update_file);
        req.open("POST", '/update/data');
    }
    else
    {
        var xmlhttp = new XMLHttpRequest();
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({command:"update_prepare","update_type":"can_by_uid","update_id":uid,"update_size":update_file_size}));
        
        formData.append(uid, update_file);
        req.open("POST", '/update/data');
    }
    
    req.upload.onprogress = function(e) {
        var p = Math.round(100 / e.total * e.loaded);
        document.getElementById(uid + "_progress").innerHTML = p + "%";
    };

    req.onload = function(e) {
        document.getElementById(uid + "_progress").innerHTML = "100%";
        var xmlhttp = new XMLHttpRequest();
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({ command:"update_complete" }));
    };
    req.send(formData);
}
