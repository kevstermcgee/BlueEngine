#!/usr/bin/env python3
"""Manage dedicated UDP game port mapping via UPnP IGD.
No router password, DMZ, firewall disable, unrelated rule deletion or permanent lease.
"""
import argparse
import socket
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET

DEFAULT_ROUTER = '192.168.0.1'
SERVICE = 'urn:schemas-upnp-org:service:WANIPConnection:1'
DESCRIPTION = 'BlueEngine game server'


def get_default_gateway():
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
            s.connect(('8.8.8.8', 80))
            local_ip = s.getsockname()[0]
            # Infer default gateway as .1 subnet address
            parts = local_ip.split('.')
            return local_ip, f"{parts[0]}.{parts[1]}.{parts[2]}.1"
    except Exception:
        return '127.0.0.1', DEFAULT_ROUTER


def soap(base_url, action, fields):
    envelope = ET.Element('{http://schemas.xmlsoap.org/soap/envelope/}Envelope')
    body = ET.SubElement(envelope, '{http://schemas.xmlsoap.org/soap/envelope/}Body')
    command = ET.SubElement(body, '{' + SERVICE + '}' + action)
    for name, value in fields.items():
        ET.SubElement(command, name).text = str(value)
    req = urllib.request.Request(
        f"{base_url}/ctl/IPConn",
        data=ET.tostring(envelope),
        headers={
            'Content-Type': 'text/xml; charset="utf-8"',
            'SOAPAction': f'"{SERVICE}#{action}"'
        }
    )
    try:
        with urllib.request.urlopen(req, timeout=5) as response:
            data = response.read(65536)
    except urllib.error.HTTPError as error:
        data = error.read(65536)
    root = ET.fromstring(data)
    values = {node.tag.rsplit('}', 1)[-1]: node.text for node in root.iter() if len(node) == 0}
    if 'errorCode' in values:
        raise RuntimeError(f"UPnP {values.get('errorCode')}: {values.get('errorDescription', '')}")
    return values


def existing(base_url, port):
    try:
        return soap(base_url, 'GetSpecificPortMappingEntry', {
            'NewRemoteHost': '',
            'NewExternalPort': str(port),
            'NewProtocol': 'UDP'
        })
    except RuntimeError as error:
        if str(error).startswith('UPnP 714:'):
            return None
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['status', 'enable', 'remove'])
    parser.add_argument('--port', type=int, default=4000, help='UDP port to forward (default: 4000)')
    parser.add_argument('--router', default=None, help='Router IP (default: auto-detect gateway)')
    parser.add_argument('--lease', type=int, default=3600, help='Lease duration in seconds (default: 3600)')
    args = parser.parse_args()

    local_ip, gateway = get_default_gateway()
    router_ip = args.router or gateway
    base_url = f"http://{router_ip}:1900"
    port_str = str(args.port)

    current = existing(base_url, args.port)
    ours = current and current.get('NewPortMappingDescription') == DESCRIPTION and current.get('NewInternalPort') == port_str

    if args.action == 'status':
        print('Local IP:', local_ip)
        print('Router IP:', router_ip)
        try:
            ext_ip = soap(base_url, 'GetExternalIPAddress', {}).get('NewExternalIPAddress')
            print('Public IPv4:', ext_ip)
        except Exception as e:
            print('Public IPv4 query failed:', e)
        print(f"UDP {args.port}:", current or 'not mapped')
        return

    if current and not ours:
        raise RuntimeError(f"UDP {args.port} belongs to another mapping; leaving it unchanged.")

    if args.action == 'remove':
        if ours:
            soap(base_url, 'DeletePortMappingEntry', {
                'NewRemoteHost': '',
                'NewExternalPort': port_str,
                'NewProtocol': 'UDP'
            })
            print(f"Removed {DESCRIPTION} UDP {args.port} mapping.")
        else:
            print(f"No existing rule owned by {DESCRIPTION}.")
        return

    # Enable
    soap(base_url, 'AddPortMapping', {
        'NewRemoteHost': '',
        'NewExternalPort': port_str,
        'NewProtocol': 'UDP',
        'NewInternalPort': port_str,
        'NewInternalClient': local_ip,
        'NewEnabled': '1',
        'NewPortMappingDescription': DESCRIPTION,
        'NewLeaseDuration': str(args.lease),
    })

    result = existing(base_url, args.port)
    if not result or result.get('NewInternalClient') != local_ip or result.get('NewEnabled') != '1':
        raise RuntimeError('Router did not confirm the requested port mapping.')
    if result.get('NewLeaseDuration') == '0':
        soap(base_url, 'DeletePortMappingEntry', {
            'NewRemoteHost': '',
            'NewExternalPort': port_str,
            'NewProtocol': 'UDP'
        })
        raise RuntimeError('Router supplied a permanent lease; removed it instead of leaving one behind.')

    print(f"Confirmed UDP {args.port} -> {local_ip}:{args.port}, lease {result.get('NewLeaseDuration')} seconds.")


if __name__ == '__main__':
    main()
