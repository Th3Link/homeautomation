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

function decimalToHex(d, padding) {
    var hex = Number(d).toString(16);
    padding = typeof (padding) === "undefined" || padding === null ? padding = 2 : padding;

    while (hex.length < padding) {
        hex = "0" + hex;
    }

    return hex.toUpperCase();
}

function build_id(uid, obj_to_replace, msg) {
    var ng = document.createElement("span");
    ng.innerHTML = uid.substring(2, 4);
    ng.style = "color:#000000";
    var type = document.createElement("span");
    type.innerHTML = uid.substring(4, 6);
    type.style = "color:#00AA00";
    var id = document.createElement("span");
    id.innerHTML = uid.substring(6, 8);
    id.style = "color:#AA0000";
    var msg_elem = document.createElement("span");
    msg_elem.innerHTML = msg
    msg_elem.style = "color:#0000AA";
    var sep = document.createElement("span");
    sep.innerHTML = "|"
    sep.style = "color:#000000";
    obj_to_replace.replaceChildren(ng,type,id,msg_elem,sep);
}

function update_relais_exec_label(uid, exec) {
    var label = document.getElementById(uid + "_label_exec_relais");
    var rtype = document.getElementById(uid + "_relais_type");
    var rnum = document.getElementById(uid + "_relais_num");
    var rstate = document.getElementById(uid + "_relais_state");
    var rtime = document.getElementById(uid + "_input_relais_time");
    
    var rtype_hex = decimalToHex(130,2);
    if (rtype.value == "rollershutter")
    {
        rtype_hex = decimalToHex(131,2);
    }

    var rstate_hex = decimalToHex(0,2);
    if ((rstate.value == "on") || (rstate.value == "up"))
    {
        rstate_hex = decimalToHex(1,2);
    }
    else if (rstate.value == "down")
    {
        rstate_hex = decimalToHex(2,2);
    }
    
    build_id(uid,label,rtype_hex);

    var num = document.createElement("span");
    num.innerHTML = decimalToHex(rnum.value,2);
    num.style = "color:#0000AA";
    var state = document.createElement("span");
    state.innerHTML = rstate_hex;
    state.style = "color:#00AA00";
    var t = decimalToHex(rtime.value,6);
    var time = document.createElement("span");
    time.innerHTML = t.substring(4,6) + t.substring(2,4) + t.substring(0,2) + " ";
    time.style = "color:#AA0000";
    label.appendChild(num);
    label.appendChild(state);
    label.appendChild(time);
    
    if (exec)
    {
        var xhr = new XMLHttpRequest();
        xhr.open("POST", '/control.json', true);
        xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        var r_command = { command: rtype.value, unit: "can_by_uid", commandId: uid ,num: parseInt(rnum.value), state: parseInt(rstate_hex, 16) , time: parseInt(rtime.value) };
        xhr.send(JSON.stringify(r_command));
    }
}

function update_lamps_exec_label(uid, exec) {
    var label = document.getElementById(uid + "_label_exec_lamps");
    var lvalue = document.getElementById(uid + "_input_lamps_value");
    
    build_id(uid,label,decimalToHex(90,2));

    var value = document.createElement("span");
    value.innerHTML = decimalToHex(lvalue.value,2);
    value.style = "color:#0000AA";
    var bitmask = document.createElement("span");
    
    var bitmask_value = 0;
    for (var i = 0; i < 24; i++)
    {
        var cb = document.getElementById(uid + "_input_lamps_bitmask_" + i);
        if (cb.checked)
        {
            bitmask_value += (1 << i);
        }
    }
    var b = decimalToHex(bitmask_value,6);
    bitmask.innerHTML = b.substring(4,6) + b.substring(2,4) + b.substring(0,2) + " ";
    bitmask.style = "color:#00AA00";
    label.appendChild(value);
    label.appendChild(bitmask);
    
    var instant = document.getElementById(uid + "_input_lamps_instant").checked;
    
    if (instant || exec)
    {
        var xhr = new XMLHttpRequest();
        xhr.open("POST", '/control.json', true);
        xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        var lamps_command = { command: "lamps", unit: "can_by_uid", commandId: uid, value: parseInt(lvalue.value), bitmask: bitmask_value };
        xhr.send(JSON.stringify(lamps_command));
    }
}

function addControls(uid) {
    var details = document.getElementById(uid + "_details").childNodes[0];
    var div = document.createElement("div");
    
    var div_relais = document.createElement("div");
    var div_relais_type = document.createElement("div");
    div_relais_type.classList.add("in_float");
    var label_relais_type = document.createElement("label");
    label_relais_type.for = uid + "_relais_type";
    label_relais_type.innerText = "Type ";
    var select_relais_type = document.createElement("select");
    select_relais_type.id = uid + "_relais_type";
    select_relais_type.name = "relais_type";

    var option_relais = document.createElement("option");
    option_relais.value = "relais";
    option_relais.innerText = "Relais";
    var option_rollershutter = document.createElement("option");
    option_rollershutter.value = "rollershutter";
    option_rollershutter.innerText = "Rollershutter";
    select_relais_type.appendChild(option_relais);
    select_relais_type.appendChild(option_rollershutter);
    
    div_relais_type.appendChild(label_relais_type);
    div_relais_type.appendChild(select_relais_type);
    div_relais.appendChild(div_relais_type);
    
    var div_relais_num = document.createElement("div");
    div_relais_num.classList.add("in_float");
    var label_relais_num = document.createElement("label");
    label_relais_num.for = uid + "_relais_num";
    label_relais_num.innerText = "No. ";
    var input_relais_num = document.createElement("input");
    input_relais_num.id = uid + "_relais_num";
    input_relais_num.name = "relais_num";
    input_relais_num.type = "text";
    input_relais_num.value = "0";
    input_relais_num.addEventListener("change", function () {
        update_relais_exec_label(uid, false);
    });
    input_relais_num.maxLength = 4;
    div_relais_num.appendChild(label_relais_num);
    div_relais_num.appendChild(input_relais_num);
    div_relais.appendChild(div_relais_num);
    
    var div_relais_state = document.createElement("div");
    div_relais_state.classList.add("in_float");
    var label_relais_state = document.createElement("label");
    label_relais_state.for = "relais_state";
    label_relais_state.innerText = "Type ";
    var select_relais_state = document.createElement("select");
    select_relais_state.id = uid + "_relais_state";
    select_relais_state.name = "relais_state";
    select_relais_state.style = "width:80px";   
    var option_on = document.createElement("option");
    option_on.value = "on";
    option_on.innerText = "ON";
    var option_off = document.createElement("option");
    option_off.value = "off";
    option_off.innerText = "OFF";
    var option_up = document.createElement("option");
    option_up.value = "up";
    option_up.innerText = "UP";
    var option_down = document.createElement("option");
    option_down.value = "down";
    option_down.innerText = "DOWN";
    
    select_relais_state.appendChild(option_off);
    select_relais_state.appendChild(option_on);
    select_relais_state.addEventListener("change", function () {
        update_relais_exec_label(uid, false);
    });
    
    div_relais_state.appendChild(label_relais_state);
    div_relais_state.appendChild(select_relais_state);
    div_relais.appendChild(div_relais_state);

    select_relais_type.addEventListener("change", function () {
        if (select_relais_type.value == "relais")
        {
            select_relais_state.replaceChildren(option_off, option_on);
            select_relais_state.value = "off";
        }
        else if (select_relais_type.value == "rollershutter")
        {
            select_relais_state.replaceChildren(option_off, option_up, option_down);
            select_relais_state.value = "off";
        }
        update_relais_exec_label(uid, false);
    });
    
    var div_relais_time = document.createElement("div");
    div_relais_time.classList.add("in_float");
    var label_relais_time = document.createElement("label");
    label_relais_time.for = uid + "_input_relais_time";
    label_relais_time.innerText = "Time (ms) ";
    var input_relais_time = document.createElement("input");
    input_relais_time.id = uid + "_input_relais_time";
    input_relais_time.name = "input_relais_time";
    input_relais_time.type = "text";
    input_relais_time.value = "0";
    input_relais_time.addEventListener("change", function () {
        update_relais_exec_label(uid, false);
    });
    input_relais_time.maxLength = 10;
    div_relais_time.appendChild(label_relais_time);
    div_relais_time.appendChild(input_relais_time);
    div_relais.appendChild(div_relais_time);
    
    var div_relais_exec = document.createElement("div");
    div_relais_exec.classList.add("last_float");
    var label_exec_relais = document.createElement("label");
    label_exec_relais.for = uid + "_exec_relais";
    label_exec_relais.id = uid + "_label_exec_relais";
    var exec_relais = document.createElement("button");
    exec_relais.id = uid + "_exec_relais";
    exec_relais.name = "exec_relais";
    exec_relais.innerText = "Exec";
    exec_relais.addEventListener("click", function () {
        update_relais_exec_label(uid, true);
    });

    div_relais_exec.appendChild(label_exec_relais);
    div_relais_exec.appendChild(exec_relais);
    div_relais.appendChild(div_relais_exec);

    var clear0 = document.createElement("div");
    clear0.classList.add("clear_float");

    // LAMPS

    var div_lamps = document.createElement("div");
    
    var div_lamps_instant = document.createElement("div");
    div_lamps_instant.classList.add("in_float");
    var label_lamps_instant = document.createElement("label");
    label_lamps_instant.for = uid + "_input_lamps_instant";
    label_lamps_instant.innerText = "Instant change ";
    var input_lamps_instant = document.createElement("input");
    input_lamps_instant.id = uid + "_input_lamps_instant";
    input_lamps_instant.name = "input_lamps_instant";
    input_lamps_instant.type = "checkbox";
    input_lamps_instant.style = "width:15px;";
    input_lamps_instant.checked = false;
    input_lamps_instant.addEventListener("change", function () {
        update_lamps_exec_label(uid,false);
    });
    
    div_lamps_instant.appendChild(label_lamps_instant);
    div_lamps_instant.appendChild(input_lamps_instant);
    div_lamps.appendChild(div_lamps_instant);
    
    var div_lamps_value = document.createElement("div");
    div_lamps_value.classList.add("in_float");
    var label_lamps_value = document.createElement("label");
    label_lamps_value.for = uid + "_input_lamps_value";
    label_lamps_value.innerText = "PWM (0-255) ";
    var input_lamps_value = document.createElement("input");
    input_lamps_value.id = uid + "_input_lamps_value";
    input_lamps_value.name = "input_lamps_value";
    input_lamps_value.type = "text";
    input_lamps_value.style = "width:30px;";
    input_lamps_value.value = "0";
    input_lamps_value.addEventListener("change", function () {
        update_lamps_exec_label(uid, false);
    });
    input_lamps_value.maxLength = 3;

    div_lamps_value.appendChild(label_lamps_value);
    div_lamps_value.appendChild(input_lamps_value);
    div_lamps.appendChild(div_lamps_value);

    var div_lamps_bitmask = document.createElement("div");
    div_lamps_bitmask.classList.add("in_float");
    for (var i = 0; i < 24; i++)
    {
        var input_lamps_bitmask = document.createElement("input");
        input_lamps_bitmask.id = uid + "_input_lamps_bitmask_" + i;
        input_lamps_bitmask.name = "input_lamps_bitmask";
        input_lamps_bitmask.type = "checkbox";
        input_lamps_bitmask.style = "width:15px;";
        input_lamps_bitmask.checked = false;
        input_lamps_bitmask.addEventListener("change", function () {
            update_lamps_exec_label(uid, false);
        });
        div_lamps_bitmask.appendChild(input_lamps_bitmask);
    }
    div_lamps.appendChild(div_lamps_bitmask);
    
    var div_lamps_exec = document.createElement("div");
    div_lamps_exec.classList.add("last_float");
    var label_exec_lamps = document.createElement("label");
    label_exec_lamps.for = uid + "_exec_lamps";
    label_exec_lamps.id = uid + "_label_exec_lamps";
    var exec_lamps = document.createElement("button");
    exec_lamps.id = uid + "_exec_lamps";
    exec_lamps.name = "exec_lamps";
    exec_lamps.innerText = "Exec";
    exec_lamps.addEventListener("click", function () {
        update_lamps_exec_label(uid, true);
    });
    div_lamps_exec.appendChild(label_exec_lamps);
    div_lamps_exec.appendChild(exec_lamps);
    div_lamps.appendChild(div_lamps_exec);

    details.appendChild(div_relais);
    details.appendChild(clear0);
    details.appendChild(div_lamps);
    update_relais_exec_label(uid);
    update_lamps_exec_label(uid);
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
    div.appendChild(c1("Firmware Version: ", uid + "_firmware"));
    div.appendChild(c1("Last Message: ", uid + "_last_message"));
    div.appendChild(c1("Device UID0: ", uid + "_uid0"));
    div.appendChild(c1("Device UID1: ", uid + "_uid1"));
    div.appendChild(c1("Baudrate: ", uid + "_baudrate"));
    div.appendChild(c1("Uptime: ", uid + "_uptime"));

    var clear0 = document.createElement("div");
    clear0.classList.add("clear_float");

    details.appendChild(div);
    var controls = createControls(uid, "in_float")
    controls.appendChild(createFirmwareSelector(uid));
    details.appendChild(controls);
    details.appendChild(clear0);
    
    details.appendChild(createRollershutterMode(uid, "in_float"));
    details.appendChild(createDeviceID(uid, "in_float"));
    details.appendChild(createDeviceType(uid, "in_float"));
    details.appendChild(createCustomString(uid, "in_float"));
    details.appendChild(createBaudrate(uid, "in_float"));
    details.appendChild(createSaveRestart(uid, "last_float"));

    var clear1 = document.createElement("div");
    clear1.classList.add("clear_float");

    details.appendChild(clear1);
    

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
        tblBody.id = "device_table_body";
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
    var tblBody = document.getElementById("device_table_body");
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
            addControls(elements[i].uid);
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
        document.getElementById(elements[i].uid + "_baudrate").textContent = elements[i].baudrate;
        document.getElementById(elements[i].uid + "_last_message").textContent = elements[i].last_seen;
        document.getElementById(elements[i].uid + "_device_id").value = elements[i].device_id;
        document.getElementById(elements[i].uid + "_device_type").value = elements[i].device_type;
        document.getElementById(elements[i].uid + "_custom_string").value = elements[i].custom_string;
        document.getElementById(elements[i].uid + "_can_baudrate").value = elements[i].baudrate;
        document.getElementById(elements[i].uid + "_device_id").dispatchEvent(new window.Event('change'));
        document.getElementById(elements[i].uid + "_device_type").dispatchEvent(new window.Event('change'));
        document.getElementById(elements[i].uid + "_custom_string").dispatchEvent(new window.Event('change'));
        document.getElementById(elements[i].uid + "_can_baudrate").dispatchEvent(new window.Event('change'));
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
    get_config(function (loaded_config) {        
        updateTypes(loaded_config);
        updateTable(loaded_config.header, loaded_config.devices);
    });
}

function add_unit(uid, command) {
    if (uid == "by_type")
    {
        var type = document.getElementById("selected_device_type").value;
        command.unit = "can_by_type";
        command.commandId = type;
    }
    else if (uid == "selected")
    {
        command.unit = "can_selected";
    }
    else if (uid == "all")
    {
        command.unit = "can_all";
    }
    else
    {
        command.unit = "can_by_uid";
        command.commandId = uid;
    }
    return command;
}

function save_click(uid) {
    var xhr = new XMLHttpRequest();
    xhr.open("POST", '/control.json', true);
    xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    var type = document.getElementById(uid+"_device_type");
    var baudrate = document.getElementById(uid+"_can_baudrate");
    var rollershutter_mode = document.getElementById(uid+"_rollershutter_mode");
    var save_command = { command: "save" };
    save_command = add_unit(uid, save_command);
    if (type.classList.contains("to_save"))
    {
        save_command.type = type.value;
    }
    if (baudrate.classList.contains("to_save"))
    {
        save_command.baudrate = baudrate.value;
    }
    if (rollershutter_mode.classList.contains("to_save"))
    {
        save_command.rollershutter_mode = rollershutter_mode.value;
    }
    if ((uid != "selected") && (uid != "by_type"))
    {
        var id = document.getElementById(uid+"_device_id");
        var custom_string = document.getElementById(uid+"_custom_string");
        if (id.classList.contains("to_save"))
        {
            save_command.id = id.value;
            save_command.type = type.value;
        }
        if (custom_string.classList.contains("to_save"))
        {
            save_command.custom_string = custom_string.value;
        }
    }
    xhr.send(JSON.stringify(save_command));
    updateDeviceList();
}

function restart_click(uid) {
    var xhr = new XMLHttpRequest();
    xhr.open("POST", '/control.json', true);
    xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    var restart_command = { command: "restart" };
    restart_command = add_unit(uid, restart_command);
    xhr.send(JSON.stringify(restart_command));
}

function ping_click(uid) {
    var xhr = new XMLHttpRequest();
    xhr.open("POST", '/control.json', true);
    xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    var ping_command = { command: "ping" };
    ping_command = add_unit(uid, ping_command);
    xhr.send(JSON.stringify(ping_command));
}

function refresh_click(uid) {
    var xhr = new XMLHttpRequest();
    xhr.open("POST", '/control.json', true);
    xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    var refresh_command = { command: "refresh" };
    refresh_command = add_unit(uid, refresh_command);
    xhr.send(JSON.stringify(refresh_command));
    xhr.onload = function(e) {
        updateTable(loaded_config.header, loaded_config.devices);
    }
    xhr.send(JSON.stringify(logging_command));
}

devices_refresh.addEventListener("click", function () {
    updateDeviceList();
    updateTypeOptions();
});

devices_broadcast_ping.addEventListener("click", function () {
    var xhr = new XMLHttpRequest();
    xhr.open("POST", '/control.json', true);
    xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    var ping_command = { command: "ping", unit: "can_all"};
    xhr.send(JSON.stringify(ping_command));
});

devices_query_all.addEventListener("click", function () {
    var xhr = new XMLHttpRequest();
    xhr.open("POST", '/control.json', true);
    xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    var refresh_command = { command: "refresh", unit: "can_all"};
    xhr.send(JSON.stringify(refresh_command));
});

devices_restart_all.addEventListener("click", function () {
    var xhr = new XMLHttpRequest();
    xhr.open("POST", '/control.json', true);
    xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
    var restart_command = { command: "restart", unit: "can_all"};
    xhr.send(JSON.stringify(restart_command));
});
