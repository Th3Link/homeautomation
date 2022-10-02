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