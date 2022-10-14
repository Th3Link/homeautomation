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
    console.log(type_options)
    var by_type_select = document.getElementById("by_type_select");
    while (by_type_select.firstChild) {
        by_type_select.removeChild(by_type_select.lastChild);
    }

    console.log(by_type_select)
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
    deviceListRequest.open('GET', 'deviceList.json');
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
