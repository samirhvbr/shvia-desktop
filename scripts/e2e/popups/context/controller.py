#!/usr/bin/env python3
"""Real XTest input against native windows; silence or cancelled default is never a pass."""
import hashlib
import json
import os
import pathlib
import ssl
import subprocess
import time
import urllib.request

ROOT = pathlib.Path('/fixture')
BASE = 'https://' + os.environ['ORIGIN_HOST'] + ':8443'
TLS = ssl.create_default_context(cafile=str(ROOT / 'ca.crt'))
SMOKE = os.environ.get('SMOKE_ONLY') == '1'
result = {'passed': False, 'smokeOnly': SMOKE, 'runKind': os.environ['RUN_KIND'], 'cases': []}
app = None


def request(route, data=None):
    payload = None if data is None else json.dumps(data).encode()
    query = urllib.request.Request(BASE + route, data=payload, headers={'content-type': 'application/json'})
    with urllib.request.urlopen(query, context=TLS, timeout=5) as response:
        return json.load(response)


def state():
    return request('/status')


def wait(predicate, seconds=20):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        if app is not None and app.poll() is not None:
            return None
        time.sleep(.15)
    return None


def save():
    (ROOT / 'acceptance-result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps({'completed': len(result['cases']), 'passedCases': sum(c.get('passed', False) for c in result['cases'])}), flush=True)


def screenshot(name):
    subprocess.run(['import', '-window', 'root', str(ROOT / name)], check=True, timeout=15)


def windows():
    command = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name', '^ShvIA$'], capture_output=True, text=True)
    return set(command.stdout.split()) if command.returncode == 0 else set()


def geometry(window):
    raw = subprocess.check_output(['xdotool', 'getwindowgeometry', '--shell', window], text=True, timeout=5)
    return {key: int(value) for key, value in (line.split('=', 1) for line in raw.splitlines())}


def focus(window):
    subprocess.run(['xdotool', 'windowactivate', window], check=True, timeout=5)
    subprocess.run(['xdotool', 'windowfocus', window], check=True, timeout=5)
    time.sleep(.15)


def click(window, rectangle):
    focus(window)
    # GTK's native menu sits above the web content; the page reports CSS rectangles.
    x = round(rectangle['x'] + rectangle['width'] / 2)
    y = round(rectangle['y'] + rectangle['height'] / 2 + 24)
    subprocess.run(['xdotool', 'mousemove', '--window', window, str(x), str(y), 'click', '1'], check=True, timeout=5)


def close(window):
    # Ask the WM to send WM_DELETE_WINDOW to this exact client. XDestroyWindow
    # bypasses CloseRequested, and a shortcut/decoration coordinate can target
    # an overlapping parent instead of the intended nested window.
    subprocess.run(['wmctrl', '-ic', hex(int(window))], check=True, timeout=5)
    return bool(wait(lambda: window not in windows(), 8))


def good(report):
    native = (report or {}).get('native') or {}
    return bool(report and report.get('hasBridge') and native.get('found') and native.get('bundled')
                and native.get('version') == os.environ['EXPECTED_ANNA_VERSION'] and not report.get('error'))


def latest(case, kind='ready'):
    return next((r for r in reversed(state()['reports']) if r['case'] == case and r['kind'] == kind), None)


def probe(case):
    command = request('/command', {'target': case})
    return wait(lambda: latest(case, 'probe-' + str(command['seq'])), 15)


def logged_event(kind, mode, offset):
    for report in reversed(state()['reports'][offset:]):
        if report['kind'] == kind:
            value = json.loads(report['text'])
            if value['mode'] == mode:
                return value
    return None


def gesture(mode, offset, activation=False):
    return wait(lambda: next((g for g in state()['gestures'][offset:] if g['mode'] == mode and g['isTrusted']
                             and (not activation or g.get('userActivation') is not False)), None), 6)


def isolated_popup(record, popup, before, return_event):
    record['popupReport'] = popup
    created = windows() - before
    record['newNativeWindows'] = sorted(created)
    record['openReturn'] = return_event
    record['passed'] = bool(record['passed'] and good(popup) and popup.get('hasOpener') is False and len(created) == 1)
    if return_event is not None:
        record['passed'] = record['passed'] and return_event['returnedNull'] is True
    if not created:
        return None
    child = next(iter(created))
    record['geometry'] = geometry(child)
    record['geometrySafe'] = record['geometry']['WIDTH'] >= 800 and record['geometry']['HEIGHT'] >= 600
    record['matchesMainDimensions'] = all(record['geometry'][axis] == result['mainGeometry'][axis] for axis in ['WIDTH', 'HEIGHT'])
    record['passed'] = record['passed'] and record['geometrySafe'] and record['matchesMainDimensions']
    return child


def child_navigation(child, case):
    outcomes = []
    for destination in ['port', 'scheme', 'host']:
        current = probe(case)
        assert good(current), 'Child bridge unavailable before navigation'
        start = len(state()['gestures'])
        click(child, current['buttons']['child-' + destination])
        url = state()['targets'][destination] + '/from-child?case=' + case + '&kind=' + destination
        event = gesture('child-' + destination, start)
        external = bool(wait(lambda: url in state()['externalOpens'], 8))
        retained = probe(case)
        outcomes.append({'destination': destination, 'url': url, 'gesture': event, 'externalHandlerObserved': external,
                         'retainedBridge': retained, 'passed': bool(event and external and good(retained))})
    return outcomes


def nested_popup(parent, parent_case, method):
    mode = 'nested-' + method
    case = parent_case + '-' + mode
    current = probe(parent_case)
    assert good(current), 'Parent bridge unavailable before nested popup'
    before, old = windows(), state()
    click(parent, current['buttons'][mode])
    event = gesture(mode, len(old['gestures']), activation=True)
    default = wait(lambda: logged_event('event-default', mode, len(old['reports'])), 3)
    opened = wait(lambda: latest(case), 15)
    returned = wait(lambda: logged_event('window-open-return', mode, len(old['reports'])), 3) if method == 'open' else None
    record = {'id': case, 'gesture': event, 'defaultAction': default,
              'passed': bool(event and default and default['defaultPreventedAfter'] is False and (method != 'open' or returned))}
    child = isolated_popup(record, opened, before, returned)
    if child:
        screenshot(case + '.png')
        record['closed'] = close(child)
    else:
        record['closed'] = False
    record['parentAfterClose'] = probe(parent_case)
    record['windowsAfterCloseMatch'] = windows() == before
    record['passed'] = bool(record['passed'] and record['closed'] and record['windowsAfterCloseMatch'] and good(record['parentAfterClose']))
    return record


try:
    result['destinationPreflight'] = []
    for destination, origin in state()['targets'].items():
        with urllib.request.urlopen(origin + '/status', context=TLS, timeout=5) as response:
            received = json.load(response)
        assert received['probeReceivedOrigin'] == origin
        result['destinationPreflight'].append({'destination': destination, 'origin': origin, 'httpStatus': 200,
                                               'receivedOrigin': received['probeReceivedOrigin'], 'role': received['probeRole']})
    result['desktopSha'] = hashlib.sha256(pathlib.Path('/usr/bin/shvia-desktop').read_bytes()).hexdigest()
    result['annaSha'] = hashlib.sha256(pathlib.Path('/usr/bin/anna').read_bytes()).hexdigest()
    result['annaVersion'] = subprocess.check_output(['/usr/bin/anna', '--version'], text=True).strip()
    assert result['annaVersion'] == os.environ['EXPECTED_ANNA_VERSION']
    result['environment'] = {'arch': subprocess.check_output(['uname', '-m'], text=True).strip(),
                             'webkit': subprocess.check_output(['dpkg-query', '-W', 'libwebkit2gtk-4.1-0'], text=True).strip(),
                             'input': 'real XTest gestures', 'network': 'internal Docker bridge', 'hostBinds': False}
    app = subprocess.Popen(['/usr/bin/shvia-desktop'], stdout=open(ROOT / 'desktop.log', 'w'), stderr=subprocess.STDOUT, start_new_session=True)
    result['initialPid'] = app.pid
    assert good(wait(lambda: latest('main'), 80)), 'Main native bridge unavailable'
    main = next(iter(windows()))
    result['mainGeometry'] = geometry(main)
    screenshot('01-main.png')
    modes = ['approved-blank', 'approved-open'] if SMOKE else [
        'port-open', 'scheme-open', 'host-open', 'port-blank', 'scheme-blank', 'host-blank', 'approved-blank', 'approved-open']
    for mode in modes:
        case = next(c for c in state()['cases'] if c['id'] == mode)
        record = {'id': mode, 'target': case['url'], 'passed': False}
        if app.poll() is not None:
            record['appExit'] = app.returncode
            result['cases'].append(record)
            save()
            continue
        current = probe('main')
        assert good(current), 'Main bridge unavailable before ' + mode
        before, old = windows(), state()
        click(main, current['buttons'][mode])
        record['gesture'] = gesture(mode, len(old['gestures']), activation=True)
        record['defaultAction'] = wait(lambda: logged_event('event-default', mode, len(old['reports'])), 3)
        returned = wait(lambda: logged_event('window-open-return', mode, len(old['reports'])), 3) if case['method'] == 'open' else None
        record['openReturn'] = returned
        record['passed'] = bool(record['gesture'] and record['defaultAction'] and record['defaultAction']['defaultPreventedAfter'] is False)
        if case['destination'] == 'approved':
            child = isolated_popup(record, wait(lambda: latest(mode), 15), before, returned)
            record['passed'] = record['passed'] and (case['method'] != 'open' or returned is not None)
            if child:
                screenshot('popup-' + mode + '.png')
                if record['passed'] and not SMOKE:
                    record['childNavigation'] = child_navigation(child, mode)
                    record['nested'] = nested_popup(child, mode, case['method'])
                    record['passed'] = record['passed'] and all(n['passed'] for n in record['childNavigation']) and record['nested']['passed']
                record['closed'] = close(child)
                record['passed'] = record['passed'] and record['closed'] and windows() == before
        else:
            record['externalHandlerObserved'] = bool(wait(lambda: case['url'] in state()['externalOpens'], 10))
            record['newNativeWindows'] = sorted(windows() - before)
            record['popupReport'] = latest(mode)
            record['passed'] = bool(record['passed'] and record['externalHandlerObserved'] and not record['newNativeWindows'] and record['popupReport'] is None)
        if app.poll() is None:
            record['parentAfterClose'] = probe('main')
            record['passed'] = bool(record['passed'] and good(record['parentAfterClose']))
        else:
            record['passed'] = False
            record['appExit'] = app.returncode
        screenshot('case-' + mode + '.png')
        result['cases'].append(record)
        save()
    final = state()
    result['unapprovedNavigationRequests'] = [r for r in final['requests'] if r['role'] == 'unapproved']
    result['unapprovedReports'] = [r for r in final['reports'] if r['role'] == 'unapproved']
    result['externalOpens'] = final['externalOpens']
    result['passed'] = bool(all(c['passed'] for c in result['cases']) and app.poll() is None
                            and not result['unapprovedNavigationRequests'] and not result['unapprovedReports'])
except Exception as error:
    result['error'] = type(error).__name__ + ': ' + str(error)
finally:
    result['appAlive'] = app is not None and app.poll() is None
    result['appExit'] = app.poll() if app else None
    try:
        screenshot('final.png')
    except subprocess.SubprocessError:
        pass
    save()
    if app and app.poll() is None:
        try:
            os.killpg(app.pid, 15)
        except OSError:
            pass
raise SystemExit(0 if result['passed'] else 1)
