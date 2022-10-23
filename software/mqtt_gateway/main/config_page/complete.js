var content_update = document.getElementById("content_update");
var content_can_device = document.getElementById("content_can_device");
var content_can_device_console = document.getElementById("content_can_device_console");
var content_state = document.getElementById("content_state");
var content_restart = document.getElementById("content_restart");

var nav_save = document.getElementById("save");
var nav_state = document.getElementById("nav_state");
var nav_can_devices = document.getElementById("nav_can_devices");
var nav_update = document.getElementById("nav_update");

var setup_general_hostname = document.getElementById("general_hostname");
var setup_wifi_mode = document.getElementById("wifi_mode");
var setup_wifi_ssid = document.getElementById("wifi_ssid");
var setup_wifi_password = document.getElementById("wifi_password");
var setup_mqtt_uri = document.getElementById("mqtt_uri");
var setup_can_baudrate = document.getElementById("can_baudrate");

var state_refresh = document.getElementById("refresh");
var state_restart = document.getElementById("restart");

var devices_refresh = document.getElementById("refresh_devices");
var devices_broadcast_ping = document.getElementById("broadcast_ping");
var devices_query_all = document.getElementById("query_all");
var devices_restart_all = document.getElementById("restart_all");

var state = null;
var loaded_config = null;
var current_config = {};

var type_options = [];

function clearPressed() {
    nav_state.classList.remove("pressed");
    nav_can_devices.classList.remove("pressed");
    nav_update.classList.remove("pressed");
    content_update.classList.add("hide-me");
    content_state.classList.add("hide-me");
    content_can_device.classList.add("hide-me");
    content_can_device_console.classList.add("hide-me");
    content_restart.classList.add("hide-me");
}

nav_can_devices.addEventListener("click", function () {
    clearPressed();
    nav_can_devices.classList.add("pressed");
    updateDeviceList();

    content_can_device.classList.remove("hide-me");
    content_can_device_console.classList.remove("hide-me");
});

nav_update.addEventListener("click", function () {
    clearPressed();
    nav_update.classList.add("pressed");
    content_update.classList.remove("hide-me");
    content_update.innerHTML = "";
    content_update.appendChild(createFirmwareSelector("device_update"));
});

setup_general_hostname.addEventListener("input", checkConfig)
setup_wifi_mode.addEventListener("input", checkConfig)
setup_wifi_ssid.addEventListener("input", checkConfig)
setup_wifi_password.addEventListener("input", checkConfig)
setup_mqtt_uri.addEventListener("input", checkConfig)
setup_can_baudrate.addEventListener("input", checkConfig)

nav_save.addEventListener("click", function () {
    if (!nav_save.classList.contains("deactivated")) {
        var xhr = new XMLHttpRequest();
        xhr.open("POST", '/control.json', true);
        xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xhr.onreadystatechange = function () {
            if (this.readyState === XMLHttpRequest.DONE && this.status === 200) {
                get_config();
                // Request finished. Do processing here.
            } else {
                //error happend
            }
        };
        xhr.send(JSON.stringify(current_config));
    }
});


state_refresh.addEventListener("click", function () {
    update_state();
    get_config();
});

state_restart.addEventListener("click", function () {
    var xhr = new XMLHttpRequest();
    xhr.open("POST", '/control.json', true);
    xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    xhr.onreadystatechange = function () {
        if (this.readyState === XMLHttpRequest.DONE && this.status === 200) {
            // Request finished. Do processing here.
            clearPressed();
            content_restart.classList.remove("hide-me");
        } else {
            //error happend
        }
    };

    const restart_command = { command: "restart", unit: "self" };

    xhr.send(JSON.stringify(restart_command));
});

function update_state() {
    var general_state = document.getElementById("general_state");
    var wifi_state = document.getElementById("wifi_state");
    var mqtt_state = document.getElementById("mqtt_state");
    var canbus_state = document.getElementById("canbus_state");

    var stateRequest = new XMLHttpRequest();
    stateRequest.open('GET', 'state.json');
    stateRequest.onload = function () {
        if (stateRequest.status >= 200 && stateRequest.status < 400) {
            state = JSON.parse(stateRequest.responseText);
            general_state.innerHTML =
                "Firmware Version: " + state.firmware_version + "<br/>" +
                "Uptime: " + state.uptime + "<br/>" +
                "Hostname: " + state.hostname;

            wifi_state.innerHTML =
                "Connection State: " + state.wifi.state + "<br/>" +
                "IPv4 Address: " + state.wifi.ipv4 + "<br/>" +
                "IPv6 Address: " + state.wifi.ipv6 + "<br/>" +
                "Gateway: " + state.wifi.gateway + "<br/>" +
                "DNS Server: " + state.wifi.dns;

            mqtt_state.innerHTML =
                "Connection State: " + state.mqtt.state + "<br/>" +
                "Messages Received: " + state.mqtt.received + "<br/>" +
                "Messages Sent: " + state.mqtt.sent;

            canbus_state.innerHTML =
                "Messages Received: " + state.mqtt.received + "<br/>" +
                "Messages Sent: " + state.mqtt.sent;
        }
    };
    stateRequest.setRequestHeader('Cache-Control', 'no-cache');
    stateRequest.send();
}

function get_config() {
    var configRequest = new XMLHttpRequest();
    configRequest.open('GET', 'state.json');
    configRequest.onload = function () {
        if (configRequest.status >= 200 && configRequest.status < 400) {

            loaded_config = JSON.parse(configRequest.responseText);
            setup_general_hostname.value = loaded_config.hostname;
            setup_wifi_mode.value = loaded_config.wifi.mode;

            setup_wifi_ssid.value = loaded_config.wifi.ssid;
            setup_wifi_password.value = loaded_config.wifi.password;

            setup_mqtt_uri.value = loaded_config.mqtt.uri;
            setup_can_baudrate.value = loaded_config.canbus.baudrate;

            checkConfig();
        }
    };
    configRequest.setRequestHeader('Cache-Control', 'no-cache');
    configRequest.send();
}

function checkConfig() {
    current_config = {};
    if (loaded_config.hostname != setup_general_hostname.value) {
        document.getElementById("general_hostname_label").classList.add("changed");
        current_config.hostname = setup_general_hostname.value;
    }
    else {
        document.getElementById("general_hostname_label").classList.remove("changed");
    }
    if (loaded_config.wifi.mode != setup_wifi_mode.value) {
        if (!('wifi' in current_config)) current_config.wifi = {};
        document.getElementById("wifi_mode_label").classList.add("changed");
        current_config.wifi.mode = setup_wifi_mode.value;
    }
    else {
        document.getElementById("wifi_mode_label").classList.remove("changed");
    }
    if (loaded_config.wifi.ssid != setup_wifi_ssid.value) {
        if (!('wifi' in current_config)) current_config.wifi = {};
        document.getElementById("wifi_ssid_label").classList.add("changed");
        current_config.wifi.ssid = setup_wifi_ssid.value;
    }
    else {
        document.getElementById("wifi_ssid_label").classList.remove("changed");
    }
    if (loaded_config.wifi.password != setup_wifi_password.value) {
        if (!('wifi' in current_config)) current_config.wifi = {};
        document.getElementById("wifi_password_label").classList.add("changed");
        current_config.wifi.password = setup_wifi_password.value;
    }
    else {
        document.getElementById("wifi_password_label").classList.remove("changed");
    }
    if (loaded_config.mqtt.uri != setup_mqtt_uri.value) {
        if (!('mqtt' in current_config)) current_config.mqtt = {};
        document.getElementById("mqtt_uri_label").classList.add("changed");
        current_config.mqtt.uri = setup_mqtt_uri.value;
    }
    else {
        document.getElementById("mqtt_uri_label").classList.remove("changed");
    }
    if (loaded_config.canbus.baudrate != setup_can_baudrate.value) {
        if (!('canbus' in current_config)) current_config.canbus = {};
        document.getElementById("can_baudrate_label").classList.add("changed");
        current_config.canbus.baudrate = setup_can_baudrate.value;
    }
    else {
        document.getElementById("can_baudrate_label").classList.remove("changed");
    }

    if (document.getElementsByClassName("changed").length != 0) {
        nav_save.classList.remove("deactivated");
    }
    else {
        nav_save.classList.add("deactivated");
    }
}

nav_state.addEventListener("click", function () {
    clearPressed();
    nav_state.classList.add("pressed");
    update_state();
    get_config();

    content_state.classList.remove("hide-me");
});

window.onload = nav_state.click();
function row_click(uid) {
    console.log("Row " + uid + "clicked!");
    var details = document.getElementById(uid + "_details");
    var other_details = document.getElementsByClassName("details");
    for (var i = 0; i < other_details.length; i++) {
        if (other_details[i] != details) {
            other_details[i].classList.add("hide-me");
        }
    }
    details.classList.toggle("hide-me");
}

function checkbox_input(uid) {
    if (uid == "all_checkbox") {
        var all_checkbox = document.getElementById("all_checkbox");
        var checkboxes = document.getElementsByClassName("checkbox");
        for (var i = 0; i < checkboxes.length; i++) {
            checkboxes[i].checked = all_checkbox.checked;
        }
    }
    console.log("Row " + uid + "clicked!");
}

function addDetails(uid) {
    function c1(s, id) {
        var div = document.createElement("div");
        var label = document.createElement("label");
        label.innerText = s;
        var value = document.createElement("label");
        value.id = id;
        div.appendChild(label);
        div.appendChild(value);
        return div;
    }
    var details = document.getElementById(uid + "_details").childNodes[0];
    var div = document.createElement("div");
    div.classList.add("state");
    div.classList.add("in_float");
    div.appendChild(c1("Firmware Version:", uid + "_firmware"));
    div.appendChild(c1("Last Message:", uid + "_last_message"));
    div.appendChild(c1("Device UID0:", uid + "_uid0"));
    div.appendChild(c1("Device UID1:", uid + "_uid1"));
    div.appendChild(c1("Baudrate:", uid + "_baudrate"));
    div.appendChild(c1("Uptime:", uid + "_uptime"));

    var clear0 = document.createElement("div");
    clear0.classList.add("clear_float");

    details.appendChild(div);
    details.appendChild(createControls(uid, "in_float"));
    details.appendChild(clear0);

    details.appendChild(createDeviceID(uid, "in_float"));
    details.appendChild(createDeviceType(uid, "in_float"));
    details.appendChild(createCustomString(uid, "in_float"));
    details.appendChild(createBaudrate(uid, "in_float"));
    details.appendChild(createSaveRestart(uid, "last_float"));

    var clear1 = document.createElement("div");
    clear1.classList.add("clear_float");

    details.appendChild(clear1);
    details.appendChild(createFirmwareSelector(uid));

    var clear1 = document.createElement("div");
    clear1.classList.add("clear_float");
    details.appendChild(clear1);

}

function createClear() {
    var clear = document.createElement("div");
    clear.classList.add("clear_float");
    return clear;
}

function createBatchView() {
    var batch = document.createElement("div");
    batch.id = "batch_content";

    var header = document.createElement("div");
    header.classList.add("header");
    header.innerText = "Batch Processing";

    var body = document.createElement("div");
    body.id = "batch_body";
    body.classList.add("hide-me");

    var selected = document.createElement("div");
    selected.id = "batch_selected";
    selected.innerHTML = "<b>Apply to selected devices</b><br/>";
    selected.appendChild(createControls("selected", "controls"));
    selected.appendChild(createClear());
    selected.appendChild(createDeviceType("selected", "in_float"));
    selected.appendChild(createBaudrate("selected", "in_float"));
    selected.appendChild(createSaveRestart("selected", "last_float"));
    selected.appendChild(createClear());
    selected.appendChild(createFirmwareSelector("selected"));

    var by_type = document.createElement("div");
    by_type.id = "batch_by_type";
    by_type.innerHTML = "<b>Apply to devices with same type (broadcast, faster)</b><br/>";

    var select = document.createElement("select");
    select.id = "by_type_select";
    select.name = "by_type_select";

    var select_c = document.createElement("div");
    select_c.classList.add("in_float");
    select_c.classList.add("control");
    select_c.appendChild(select);
    by_type.appendChild(select_c);
    by_type.appendChild(createControls("by_type", "last_float"));

    by_type.appendChild(createClear());

    by_type.appendChild(createDeviceType("by_type", "in_float"));
    by_type.appendChild(createBaudrate("by_type", "in_float"));
    by_type.appendChild(createSaveRestart("by_type", "last_float"));

    by_type.appendChild(createClear());
    by_type.appendChild(createFirmwareSelector("by_type"));

    batch.appendChild(header);
    batch.appendChild(body);

    body.appendChild(selected);
    body.appendChild(by_type);
    body.appendChild(createClear());

    header.addEventListener("click", function () {
        var bb = document.getElementById("batch_body");
        bb.classList.toggle("hide-me");
    });

    return batch;
}

function updateTable(header, elements) {
    const tableElements = 9;

    var tbl = document.getElementById("device_table");
    if (tbl == null) {


        tbl = document.createElement("table");
        var tblBody = document.createElement("tbody");
        var header_tr = document.createElement("tr");
        header_tr.classList.add("header")
        var l = header.length;

        var cell = document.createElement("th");
        var checkbox = document.createElement("input");
        checkbox.type = "checkbox";
        checkbox.id = "all_checkbox";
        checkbox.addEventListener("input", function () {
            checkbox_input("all_checkbox");
        });
        cell.appendChild(checkbox);
        header_tr.appendChild(cell);

        for (var i = 0; i < l; i++) {
            var th = document.createElement("th");
            var th_text = document.createTextNode(header[i]);
            th.appendChild(th_text);
            header_tr.appendChild(th);
        }
        tblBody.appendChild(header_tr);
        tbl.id = "device_table";
        tbl.appendChild(tblBody);
        content_can_device.innerHTML = "";
        content_can_device.appendChild(createBatchView());
        content_can_device.appendChild(tbl);
    }

    var l = elements.length;
    for (var i = 0; i < l; i++) {
        let element_uid = elements[i].uid;
        var device_tr = document.getElementById(elements[i].uid);
        if (device_tr == null) {
            device_tr = document.createElement("tr");
            device_tr.id = elements[i].uid;

            var cell = document.createElement("td");
            var checkbox = document.createElement("input");
            checkbox.type = "checkbox";
            checkbox.id = elements[i].uid + "_checkbox";
            checkbox.classList.add("checkbox");
            checkbox.addEventListener("input", function () {
                checkbox_input(element_uid);
            });

            cell.appendChild(checkbox);
            device_tr.appendChild(cell);

            for (var j = 1; j < tableElements; j++) {
                var cell = document.createElement("td");
                var cellText = document.createTextNode("");
                cell.addEventListener("click", function () {
                    row_click(element_uid);
                });
                cell.appendChild(cellText);
                device_tr.appendChild(cell);
            }
            device_details_tr = document.createElement("tr");
            device_details_tr.classList.add("hide-me");
            device_details_tr.classList.add("details");
            device_details_tr.id = elements[i].uid + "_details";
            device_details_td = document.createElement("td");
            device_details_td.colSpan = tableElements;
            device_details_tr.appendChild(device_details_td);
            tblBody.appendChild(device_tr);
            tblBody.appendChild(device_details_tr);
            addDetails(elements[i].uid);
        }

        updateTypeOptions();

        device_tr.childNodes[1].childNodes[0].textContent = elements[i].uid;
        device_tr.childNodes[2].childNodes[0].textContent = elements[i].device_id;
        device_tr.childNodes[3].childNodes[0].textContent = elements[i].device_type;
        device_tr.childNodes[4].childNodes[0].textContent = elements[i].device_type_name;
        device_tr.childNodes[5].childNodes[0].textContent = elements[i].custom_string;
        device_tr.childNodes[6].childNodes[0].textContent = elements[i].last_seen;
        device_tr.childNodes[7].childNodes[0].textContent = elements[i].state;
        device_tr.childNodes[8].childNodes[0].textContent = elements[i].last_error;
        
        document.getElementById(elements[i].uid + "_firmware").textContent = elements[i].version;
        document.getElementById(elements[i].uid + "_uid0").textContent = elements[i].uid0;
        document.getElementById(elements[i].uid + "_uid1").textContent = elements[i].uid1;
        document.getElementById(elements[i].uid + "_uptime").textContent = elements[i].uptime;
        document.getElementById(elements[i].uid + "_last_message").textContent = elements[i].last_seen;
    }
}

function typeInOptions(p_type_options, type) {
    for (var i = 0; i < p_type_options.length; i++) {
        if (type == p_type_options[i].index) {
            return true;
        }
    }
    return false;
}

function updateTypes(device_list) {
    type_options = []

    for (var i = 0; i < device_list.devices.length; i++) {
        if (!typeInOptions(type_options, device_list.devices[i].device_type)) {
            type_options.push({
                index: device_list.devices[i].device_type,
                name: device_list.devices[i].device_type_name
            });
        }
    }
}

function updateTypeOptions() {
    var by_type_select = document.getElementById("by_type_select");
    while (by_type_select.firstChild) {
        by_type_select.removeChild(by_type_select.lastChild);
    }

    if (by_type_select != null) {
        for (i = 0; i < type_options.length; i++) {
            var option = document.createElement("option");
            option.value = type_options[i].index;
            option.innerText = type_options[i].name + " (" + type_options[i].index + ")";
            by_type_select.appendChild(option);
        }
    }
}

function updateDeviceList() {
    var deviceListRequest = new XMLHttpRequest();
    deviceListRequest.open('GET', 'state.json');
    deviceListRequest.onload = function () {
        if (deviceListRequest.status >= 200 && deviceListRequest.status < 400) {
            device_list = JSON.parse(deviceListRequest.responseText);
            updateTypes(device_list);
            updateTable(device_list.header, device_list.devices);
        }
    };
    deviceListRequest.setRequestHeader('Cache-Control', 'no-cache');
    deviceListRequest.send();
}

function save_click(uid) {
    console.log("save " + uid);
}

function restart_click(uid) {
    console.log("restart " + uid);
}

function ping_click(uid) {
    console.log("ping " + uid);
}

function refresh_click(uid) {
    console.log("refresh " + uid);
}

devices_refresh.addEventListener("click", function () {
    updateDeviceList();
    updateTypeOptions();
});

devices_broadcast_ping.addEventListener("click", function () {

});

devices_query_all.addEventListener("click", function () {

});

devices_restart_all.addEventListener("click", function () {

});
function createDeviceID(uid, cl) {
    var div = document.createElement("div");
    div.classList.add(cl);
    var label = document.createElement("label");
    label.for = "device_id";
    label.innerText = "Device ID ";
    var input = document.createElement("input");
    input.id = uid + "_device_id";
    input.name = "device_id";
    input.type = "text";
    div.appendChild(label);
    div.appendChild(input);
    return div;
}

function createDeviceType(uid, cl) {
    var div = document.createElement("div");
    div.classList.add(cl);
    var label = document.createElement("label");
    label.for = "device_type";
    label.innerText = "Device Type ";
    var input = document.createElement("input");
    input.id = uid + "_device_type";
    input.name = "device_type";
    input.type = "text";
    div.appendChild(label);
    div.appendChild(input);
    return div;
}

function createCustomString(uid, cl) {
    var div = document.createElement("div");
    div.classList.add(cl);
    var label = document.createElement("label");
    label.for = "custom_string";
    label.innerText = "Custom String ";
    var input = document.createElement("input");
    input.id = uid + "_custom_string";
    input.name = "custom_string";
    input.type = "text";
    input.maxLength = 8;
    div.appendChild(label);
    div.appendChild(input);
    return div;
}

function createBaudrate(uid, cl) {
    var div = document.createElement("div");
    div.classList.add(cl);
    div.classList.add("canbus_setup");
    var label = document.createElement("label");
    label.for = "baudrate";
    label.innerText = "Baudrate ";
    var select = document.createElement("select");
    select.id = uid + "_can_baudrate";
    select.name = "can_baudrate";

    var optionb50 = document.createElement("option");
    optionb50.value = "b50";
    optionb50.innerText = "50 KBit/s";
    var optionb22_222 = document.createElement("option");
    optionb22_222.value = "b22_222";
    optionb22_222.innerText = "22.222 KBit/s";
    var optionb25 = document.createElement("option");
    optionb25.value = "b25";
    optionb25.innerText = "25 KBit/s";
    var optionb100 = document.createElement("option");
    optionb100.value = "b100";
    optionb100.innerText = "100 KBit/s";
    select.appendChild(optionb50);
    select.appendChild(optionb22_222);
    select.appendChild(optionb25);
    select.appendChild(optionb100);
    div.appendChild(label);
    div.appendChild(select);
    return div;
}

function createSaveRestart(uid, cl) {
    var div = document.createElement("div");
    div.classList.add(cl);
    var save = document.createElement("button");
    save.id = uid + "_save";
    save.name = "save";
    save.innerText = "Save";
    save.addEventListener("click", function () {
        save_click(uid);
    });
    div.appendChild(save);
    return div;
}

function createFirmwareSelector(uid) {
    var div = document.createElement("div");

    var desc = document.createElement("label");
    desc.for = "file";
    desc.innerText = "Choose file to upload ";

    var upload = document.createElement("input");
    upload.id = uid + "_file_upload";
    upload.name = "file";
    upload.type = "file";
    upload.classList.add("upload");
    upload.accept = ".bin";

    var button = document.createElement("button");
    button.id = uid + "_update";
    button.name = "update";
    button.innerText = "Update";
    button.addEventListener("click", function () {
        update_click(uid);
    });

    var label = document.createElement("label");
    label.id = uid + "_progress";
    label.name = "progress";
    label.innerText = "";

    div.appendChild(desc);
    div.appendChild(upload);
    div.appendChild(button);
    div.appendChild(label);

    return div;
}
function createControls(uid, cl) {
    var control = document.createElement("div");
    control.classList.add("control");
    if (cl.length > 0)
        control.classList.add(cl);

    var refresh = document.createElement("button");
    refresh.id = uid + "_refresh";
    refresh.name = "refresh";
    refresh.innerText = "Refresh";
    refresh.addEventListener("click", function () {
        refresh_click(uid);
    });

    var restart = document.createElement("button");
    restart.id = uid + "_restart";
    restart.name = "restart";
    restart.innerText = "Restart";
    restart.addEventListener("click", function () {
        restart_click(uid);
    });

    var ping = document.createElement("button");
    ping.id = uid + "_ping";
    ping.name = "ping";
    ping.innerText = "Ping";
    ping.addEventListener("click", function () {
        ping_click(uid);
    });

    control.appendChild(refresh);
    control.appendChild(restart);
    control.appendChild(ping);

    return control;
}
function update_click(uid) {
    console.log("update " + uid);
    var update_file = document.getElementById(uid + "_file_upload").files[0];
    var req = new XMLHttpRequest();
    var formData = new FormData();

    if (uid == "by_type")
    {
        var type = document.getElementById("selected_device_type").value;
        console.log(type);
        
        var xmlhttp = new XMLHttpRequest();
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({"update_prepare":true,"update_type":"can_by_type","update_id":type}));
        
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
        xmlhttp.send(JSON.stringify({"update_prepare":true,"update_type":"self"}));
        
        formData.append("device_update", update_file);
        req.open("POST", '/update/data');
    }
    else
    {
        var xmlhttp = new XMLHttpRequest();
        xmlhttp.open("POST", "/control.json");
        xmlhttp.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        xmlhttp.send(JSON.stringify({"update_prepare":true,"update_type":"can_by_uid","update_id":uid}));
        
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
        xmlhttp.send(JSON.stringify({ "update_complete": true }));
    };
    req.send(formData);
}
