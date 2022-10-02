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
    configRequest.open('GET', 'config.json');
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