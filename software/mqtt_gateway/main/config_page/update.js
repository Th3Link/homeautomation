function update_click(uid) {
    var update_file = document.getElementById(uid + "_file_upload").files[0];
    if ((typeof update_file == 'undefined') || update_file == null)
    {
        return;
    }
    var update_file_size = document.getElementById(uid + "_file_upload").files[0].size;
    var req = new XMLHttpRequest();
    var formData = new FormData();
    var reload = false;
    if (uid == "by_type")
    {
        var type = document.getElementById("selected_device_type").value;
        
        var xmlhttp = new XMLHttpRequest();
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({command:"update_prepare","update_type":"can_by_type","update_id":type,"update_size":update_file_size}));
        xmlhttp.onload = function(e) {
            req.send(formData);
        }
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
        xmlhttp.onload = function(e) {
            req.send(formData);
        }
        formData.append("device_update", update_file);
        req.open("POST", '/update/data');
        reload = true;
    }
    else
    {
        var xmlhttp = new XMLHttpRequest();
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({command:"update_prepare","update_type":"can_by_uid","update_id":uid,"update_size":update_file_size}));
        xmlhttp.onload = function(e) {
            req.send(formData);
        }
        formData.append(uid, update_file);
        req.open("POST", '/update/data');
    }
    
    req.upload.onprogress = function(e) {
        var p = Math.round(100 / e.total * e.loaded);
        document.getElementById(uid + "_progress").innerHTML = p + "%";
    };
    
    var xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function(e) {
        document.getElementById(uid + "_progress").innerHTML = "Update complete";
    }
    
    req.onload = function(e) {
        document.getElementById(uid + "_progress").innerHTML = "100%";
        
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");

        if (reload) {        
            document.getElementById(uid + "_progress").innerHTML = "Update complete, restarting...";
            setTimeout(function() {
                 location.reload();
            }, 8000);
        }
        xmlhttp.send(JSON.stringify({ command:"update_complete" }));
    };
    
}

function bridge_config_upload_click(uid) {
    var bridge_config_file = document.getElementById(uid + "_bridge_congig_upload").files[0];
    if ((typeof bridge_config_file == 'undefined') || bridge_config_file == null)
    {
        return;
    }
    var req = new XMLHttpRequest();
    var formData = new FormData();

    var xmlhttp = new XMLHttpRequest();
    xmlhttp.open("POST", "/control.json");
    xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    xmlhttp.send(JSON.stringify({command:"bridge_config_prepare","bridge_config_size":bridge_config_file.size}));
    xmlhttp.onload = function(e) {
        req.send(formData);
    }
    formData.append("bridge_congig_upload", bridge_config_file);
    req.open("POST", '/bride_config');
    
    req.upload.onprogress = function(e) {
        var p = Math.round(100 / e.total * e.loaded);
        document.getElementById(uid + "_progress").innerHTML = p + "%";
    };
    
    var xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function(e) {
        document.getElementById(uid + "_progress").innerHTML = "Upload complete";
    }
    
    req.onload = function(e) {
        document.getElementById(uid + "_progress").innerHTML = "100%";
        
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
     
        document.getElementById(uid + "_progress").innerHTML = "Bridge config upload complete, appliing...";
        setTimeout(function() {
             location.reload();
        }, 8000);
        
        xmlhttp.send(JSON.stringify({ command:"bridge_config_complete" }));
    };
    
}
