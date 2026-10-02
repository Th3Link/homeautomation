#!/usr/bin/env python3

from klein import run, route, Klein
from twisted.web.static import File
import json
import collections.abc

def update(d, u):
    for k, v in u.items():
        if isinstance(v, collections.abc.Mapping):
            d[k] = update(d.get(k, {}), v)
        else:
            d[k] = v
    return d

app = Klein()
global config
config = None

@app.route('/config.json', methods=['GET'])
def config_json(request):
    global config
    if config is None:
        with open('config.json', 'r') as config_file:
            config = json.loads(config_file.read())
    print(config)
    return json.dumps(config)

@app.route('/config.json', methods=['POST'])
def config_json_post(request):
    global config
    if config is None:
        with open('config.json', 'r') as config_file:
            config = json.loads(config_file.read())
    d = json.loads(request.content.read())
    update(config, d)
    return json.dumps(config)

@app.route('/update/device/data', methods=['POST'])
def device_update_post(request):
    with open('device_update', 'wb') as fileOutput:
        fileOutput.write(request.content.read())

@app.route('/update/device/state', methods=['POST'])
def device_update_post(request):
    print(request.content)

@app.route('/<string:filename>', branch=True, methods=['POST'])
def do_post(request, filename):
    with open('POST_'+filename, 'wb') as fileOutput:
        fileOutput.write(request.content.read())

@app.route('/', branch=True, methods=['GET'])
def pg_index(request):
    return File('./')

app.run("localhost", 8080)
