Feature: Model Context Protocol interface
  As an MCP client
  I want a standards-compliant server
  So that any agent framework can use Ommatix without custom glue

  # Resumen (ES): el servidor cumple el subconjunto de MCP/JSON-RPC que usa el MVP.

  Background:
    Given the simulated desktop is running

  Scenario: The handshake negotiates the protocol version
    When the client initializes with protocol version "2025-03-26"
    Then the server answers with protocol version "2025-03-26"
    And the server identifies itself as "ommatix"

  Scenario: The tool catalogue covers perception and actuation
    When the client lists the tools
    Then the tools are:
      | perceive         |
      | list_elements    |
      | element_at       |
      | elements_in_rect |
      | click            |
      | type_text        |
      | press_key        |

  Scenario Outline: Protocol errors use JSON-RPC error codes
    When the client sends <message>
    Then the response is a JSON-RPC error <code>

    Examples:
      | message                                                                 | code   |
      | {not json                                                               | -32700 |
      | 42                                                                      | -32600 |
      | []                                                                      | -32600 |
      | {"jsonrpc":"1.0","id":1,"method":"ping"}                                | -32600 |
      | {"jsonrpc":"2.0","id":1,"method":"resources/list"}                      | -32601 |
      | {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nope"}} | -32602 |

  Scenario: A batch gets one response per request and none per notification
    When the client sends [{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"tools/list"}]
    Then the response is a batch of 2 responses

  Scenario: Invalid tool arguments are reported as a tool error the agent can fix
    When the agent calls "element_at" with {"x": 1}
    Then the call fails with "invalid arguments"
