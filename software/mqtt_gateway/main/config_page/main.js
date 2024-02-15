var content_update = document.getElementById("content_update");
var content_can_device = document.getElementById("content_can_device");
var content_can_device_console = document.getElementById("content_can_device_console");
var content_bridge = document.getElementById("content_bridge");
var content_state = document.getElementById("content_state");
var content_restart = document.getElementById("content_restart");
var content_logging = document.getElementById("content_logging");
var content_docs = document.getElementById("content_docs");

var nav_save = document.getElementById("save");
var nav_state = document.getElementById("nav_state");
var nav_can_devices = document.getElementById("nav_can_devices");
var nav_bridge = document.getElementById("nav_bridge");
var nav_update = document.getElementById("nav_update");
var nav_logging = document.getElementById("nav_logging");
var nav_docs = document.getElementById("nav_docs");

var setup_general_hostname = document.getElementById("general_hostname");
var setup_general_username = document.getElementById("web_username");
var setup_general_password = document.getElementById("web_password");
var setup_wifi_mode = document.getElementById("wifi_mode");
var setup_wifi_ssid = document.getElementById("wifi_ssid");
var setup_wifi_password = document.getElementById("wifi_password");
var setup_mqtt_uri = document.getElementById("mqtt_uri");
var setup_mqtt_username = document.getElementById("mqtt_username");
var setup_mqtt_password = document.getElementById("mqtt_password");
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
    nav_bridge.classList.remove("pressed");
    nav_update.classList.remove("pressed");
    nav_logging.classList.remove("pressed");
    nav_docs.classList.remove("pressed");
    content_update.classList.add("hide-me");
    content_state.classList.add("hide-me");
    content_can_device.classList.add("hide-me");
    content_can_device_console.classList.add("hide-me");
    content_restart.classList.add("hide-me");
    content_logging.classList.add("hide-me");
    content_docs.classList.add("hide-me");
    content_bridge.classList.add("hide-me");
}
function init() {
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

    nav_bridge.addEventListener("click", function () {
        clearPressed();
        nav_bridge.classList.add("pressed");
        content_bridge.classList.remove("hide-me");
        content_bridge.innerHTML = "";
        content_bridge.appendChild(createBridgeSelector("bridge_config"));
    });

    nav_logging.addEventListener("click", function () {
        clearPressed();
        nav_logging.classList.add("pressed");
        content_logging.classList.remove("hide-me");
        content_logging.innerHTML = "";
    });

    nav_docs.addEventListener("click", function () {
        clearPressed();
        nav_docs.classList.add("pressed");
        content_docs.classList.remove("hide-me");
        content_docs.innerHTML = "";
    });

    setup_general_hostname.addEventListener("input", checkConfig)
    setup_general_username.addEventListener("input", checkConfig)
    setup_general_password.addEventListener("input", checkConfig)
    setup_wifi_mode.addEventListener("input", checkConfig)
    setup_wifi_ssid.addEventListener("input", checkConfig)
    setup_wifi_password.addEventListener("input", checkConfig)
    setup_mqtt_uri.addEventListener("input", checkConfig)
    setup_mqtt_username.addEventListener("input", checkConfig)
    setup_mqtt_password.addEventListener("input", checkConfig)
    setup_can_baudrate.addEventListener("input", checkConfig)

    var label_mqtt_logging = document.createElement("div");
    label_mqtt_logging.classList.add("label");
    label_mqtt_logging.id = "label_mqtt_logging";
    label_mqtt_logging.innerText = "MQTT Logging ";
    var input_mqtt_logging = document.createElement("input");
    input_mqtt_logging.id = "input_mqtt_logging";
    input_mqtt_logging.name = "input_mqtt_logging";
    input_mqtt_logging.type = "checkbox";
    input_mqtt_logging.style = "width:15px;margin-right:20px;";
    input_mqtt_logging.checked = false;
    input_mqtt_logging.addEventListener("change", function () {
        var xhr = new XMLHttpRequest();
        xhr.open("POST", '/control.json', true);
        xhr.setRequestHeader("Content-Type", "application/json;charset=UTF-8");
        var logging_command = { command: "mqtt_logging", unit: "can_all", enabled: input_mqtt_logging.checked };
        xhr.onload = function(e) {
            get_config();
        }
        xhr.send(JSON.stringify(logging_command));
    });
    document.getElementById("canbus_setup").appendChild(label_mqtt_logging);
    document.getElementById("canbus_setup").appendChild(input_mqtt_logging);
    
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
            current_config.command = "save_config"
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
    
    nav_state.addEventListener("click", function () {
        clearPressed();
        nav_state.classList.add("pressed");
        update_state();
        get_config();

        content_state.classList.remove("hide-me");
    });
    
    nav_state.click();
}

function get_config(onload = null) {
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
            setup_mqtt_username.value = loaded_config.mqtt.username;
            setup_mqtt_password.value = loaded_config.mqtt.password;
            setup_can_baudrate.value = loaded_config.canbus.baudrate;

            checkConfig();
            if (onload != null)
            {
                onload(loaded_config);
            }
        }
    };
    configRequest.setRequestHeader('Cache-Control', 'no-cache');
    configRequest.send();
}

function checkConfig() {
    current_config = {};
    
    document.getElementById("input_mqtt_logging").checked = loaded_config.canbus.mqtt_logging;
    
    if (setup_general_username.value != loaded_config.username) {
        document.getElementById("web_username_label").classList.add("changed");
        current_config.username = setup_general_username.value;
    }
    else
    {
        document.getElementById("web_username_label").classList.remove("changed");
        delete current_config.username;
    }
    if (setup_general_password.value != loaded_config.password) {
        document.getElementById("web_password_label").classList.add("changed");
        current_config.password = setup_general_password.value;
    }
    else
    {
        document.getElementById("web_password_label").classList.remove("changed");
        delete current_config.password;
    }
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
    if (loaded_config.mqtt.username != setup_mqtt_username.value) {
        if (!('mqtt' in current_config)) current_config.mqtt = {};
        document.getElementById("mqtt_username_label").classList.add("changed");
        current_config.mqtt.username = setup_mqtt_username.value;
    }
    else {
        document.getElementById("mqtt_username_label").classList.remove("changed");
    }
    if (loaded_config.mqtt.password != setup_mqtt_password.value) {
        if (!('mqtt' in current_config)) current_config.mqtt = {};
        document.getElementById("mqtt_password_label").classList.add("changed");
        current_config.mqtt.password = setup_mqtt_password.value;
    }
    else {
        document.getElementById("mqtt_password_label").classList.remove("changed");
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

function get_device_object(uid)
{
    var dev = 0;
    loaded_config.devices.forEach(function (device, index) {
        if (device.uid == uid)
        {
            dev = device;
        }
    });
    return dev;
}

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
                "Messages Received: " + state.canbus.received + "<br/>" +
                "Messages Sent: " + state.canbus.sent;
        }
    };
    stateRequest.setRequestHeader('Cache-Control', 'no-cache');
    stateRequest.send();
}

window.onload = init();
