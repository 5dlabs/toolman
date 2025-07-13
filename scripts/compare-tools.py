#!/usr/bin/env python3
"""
Tool Configuration Comparison Script

Compares discovered tools (from --export-tools) against configured tools
in servers-config.json to identify discrepancies and maintain accuracy.
"""

import json
import sys
from pathlib import Path
from typing import Dict, List, Set, Tuple

def load_discovered_tools(filepath: str) -> Dict[str, List[str]]:
    """Load discovered tools from export file."""
    with open(filepath, 'r') as f:
        data = json.load(f)

    discovered = {}
    for server in data['servers']:
        server_name = server['name']
        tool_names = [tool['name'] for tool in server['tools']]
        discovered[server_name] = tool_names

    return discovered

def load_configured_tools(filepath: str) -> Dict[str, List[str]]:
    """Load configured tools from servers-config.json."""
    with open(filepath, 'r') as f:
        data = json.load(f)

    configured = {}
    for server_name, server_config in data['servers'].items():
        if 'tools' in server_config:
            tool_names = list(server_config['tools'].keys())
            configured[server_name] = tool_names
        else:
            configured[server_name] = []

    return configured

def compare_tools(discovered: Dict[str, List[str]], configured: Dict[str, List[str]]) -> Dict:
    """Compare discovered vs configured tools and return analysis."""
    analysis = {
        'summary': {
            'total_servers_discovered': len(discovered),
            'total_servers_configured': len(configured),
            'total_tools_discovered': sum(len(tools) for tools in discovered.values()),
            'total_tools_configured': sum(len(tools) for tools in configured.values()),
        },
        'servers': {}
    }

    all_servers = set(discovered.keys()) | set(configured.keys())

    for server_name in sorted(all_servers):
        disc_tools = set(discovered.get(server_name, []))
        conf_tools = set(configured.get(server_name, []))

        missing_from_config = disc_tools - conf_tools
        extra_in_config = conf_tools - disc_tools
        matching_tools = disc_tools & conf_tools

        server_analysis = {
            'discovered_count': len(disc_tools),
            'configured_count': len(conf_tools),
            'matching_count': len(matching_tools),
            'missing_from_config': sorted(list(missing_from_config)),
            'extra_in_config': sorted(list(extra_in_config)),
            'matching_tools': sorted(list(matching_tools)),
            'status': 'unknown'
        }

        # Determine server status
        if server_name not in discovered:
            server_analysis['status'] = 'configured_but_not_discovered'
        elif server_name not in configured:
            server_analysis['status'] = 'discovered_but_not_configured'
        elif len(disc_tools) == 0:
            server_analysis['status'] = 'no_tools_discovered'
        elif len(missing_from_config) == 0 and len(extra_in_config) == 0:
            server_analysis['status'] = 'perfect_match'
        elif len(missing_from_config) > 0 and len(extra_in_config) == 0:
            server_analysis['status'] = 'missing_tools'
        elif len(missing_from_config) == 0 and len(extra_in_config) > 0:
            server_analysis['status'] = 'extra_tools'
        else:
            server_analysis['status'] = 'mixed_differences'

        analysis['servers'][server_name] = server_analysis

    return analysis

def print_summary(analysis: Dict):
    """Print a summary of the comparison."""
    summary = analysis['summary']

    print("🔍 TOOL CONFIGURATION COMPARISON SUMMARY")
    print("=" * 50)
    print(f"📊 Servers: {summary['total_servers_discovered']} discovered, {summary['total_servers_configured']} configured")
    print(f"🛠️  Tools: {summary['total_tools_discovered']} discovered, {summary['total_tools_configured']} configured")
    print()

    # Count servers by status
    status_counts = {}
    for server_data in analysis['servers'].values():
        status = server_data['status']
        status_counts[status] = status_counts.get(status, 0) + 1

    print("📈 SERVER STATUS BREAKDOWN:")
    status_emojis = {
        'perfect_match': '✅',
        'missing_tools': '⚠️ ',
        'extra_tools': '🔧',
        'mixed_differences': '🔄',
        'no_tools_discovered': '❌',
        'discovered_but_not_configured': '🆕',
        'configured_but_not_discovered': '👻'
    }

    for status, count in sorted(status_counts.items()):
        emoji = status_emojis.get(status, '❓')
        print(f"  {emoji} {status.replace('_', ' ').title()}: {count}")

    print()

def print_detailed_analysis(analysis: Dict):
    """Print detailed analysis for each server."""
    print("📋 DETAILED SERVER ANALYSIS")
    print("=" * 50)

    for server_name, server_data in analysis['servers'].items():
        status = server_data['status']
        emoji = {
            'perfect_match': '✅',
            'missing_tools': '⚠️ ',
            'extra_tools': '🔧',
            'mixed_differences': '🔄',
            'no_tools_discovered': '❌',
            'discovered_but_not_configured': '🆕',
            'configured_but_not_discovered': '👻'
        }.get(status, '❓')

        print(f"\n{emoji} **{server_name}** ({status.replace('_', ' ')})")
        print(f"   Discovered: {server_data['discovered_count']} tools")
        print(f"   Configured: {server_data['configured_count']} tools")
        print(f"   Matching: {server_data['matching_count']} tools")

        if server_data['missing_from_config']:
            print(f"   🔍 Missing from config ({len(server_data['missing_from_config'])}):")
            for tool in server_data['missing_from_config']:
                print(f"      + {tool}")

        if server_data['extra_in_config']:
            print(f"   ❓ Extra in config ({len(server_data['extra_in_config'])}):")
            for tool in server_data['extra_in_config']:
                print(f"      - {tool}")

def print_actionable_recommendations(analysis: Dict):
    """Print actionable recommendations based on the analysis."""
    print("\n🎯 ACTIONABLE RECOMMENDATIONS")
    print("=" * 50)

    recommendations = []

    # Find servers with missing tools
    servers_missing_tools = []
    servers_with_extra_tools = []
    servers_not_configured = []
    broken_servers = []

    for server_name, server_data in analysis['servers'].items():
        if server_data['status'] == 'missing_tools' or server_data['status'] == 'mixed_differences':
            if server_data['missing_from_config']:
                servers_missing_tools.append((server_name, len(server_data['missing_from_config'])))

        if server_data['status'] == 'extra_tools' or server_data['status'] == 'mixed_differences':
            if server_data['extra_in_config']:
                servers_with_extra_tools.append((server_name, len(server_data['extra_in_config'])))

        if server_data['status'] == 'discovered_but_not_configured':
            servers_not_configured.append((server_name, server_data['discovered_count']))

        if server_data['status'] == 'no_tools_discovered':
            broken_servers.append(server_name)

    if servers_missing_tools:
        print("1. 📝 UPDATE CONFIG - Add missing tools:")
        for server, count in sorted(servers_missing_tools, key=lambda x: x[1], reverse=True):
            print(f"   • {server}: {count} missing tools")

    if servers_with_extra_tools:
        print("\n2. 🧹 CLEAN CONFIG - Remove non-existent tools:")
        for server, count in sorted(servers_with_extra_tools, key=lambda x: x[1], reverse=True):
            print(f"   • {server}: {count} extra tools")

    if servers_not_configured:
        print("\n3. 🆕 ADD SERVERS - Configure newly discovered servers:")
        for server, count in sorted(servers_not_configured, key=lambda x: x[1], reverse=True):
            print(f"   • {server}: {count} tools available")

    if broken_servers:
        print("\n4. 🔧 FIX BROKEN SERVERS - These servers discovered 0 tools:")
        for server in sorted(broken_servers):
            print(f"   • {server}")

    print(f"\n💡 TIP: Use the exported tool schemas in 'discovered-tools.json' to update your configuration!")

def main():
    if len(sys.argv) != 3:
        print("Usage: python compare-tools.py <discovered-tools.json> <servers-config.json>")
        sys.exit(1)

    discovered_file = sys.argv[1]
    config_file = sys.argv[2]

    if not Path(discovered_file).exists():
        print(f"Error: {discovered_file} not found")
        sys.exit(1)

    if not Path(config_file).exists():
        print(f"Error: {config_file} not found")
        sys.exit(1)

    try:
        discovered = load_discovered_tools(discovered_file)
        configured = load_configured_tools(config_file)
        analysis = compare_tools(discovered, configured)

        print_summary(analysis)
        print_detailed_analysis(analysis)
        print_actionable_recommendations(analysis)

        # Save detailed analysis to file
        output_file = "tool-comparison-analysis.json"
        with open(output_file, 'w') as f:
            json.dump(analysis, f, indent=2)

        print(f"\n📄 Detailed analysis saved to: {output_file}")

    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()