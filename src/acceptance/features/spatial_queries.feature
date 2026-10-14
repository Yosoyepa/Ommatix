Feature: Spatial queries with stable identities
  As an agent
  I want exact coordinates and stable ids for what is on screen
  So that I can target elements deterministically across steps

  # Resumen (ES): consultas por punto devuelven límites exactos e ids que no cambian.

  Background:
    Given the simulated desktop is running
    And the agent perceives the screen

  Scenario Outline: Buttons are reported at their exact on-screen bounds
    When the agent asks what is under the "<button>" button
    Then the answer is the "<button>" button with its exact bounds

    Examples:
      | button |
      | Save   |
      | Cancel |

  Scenario: The smallest element under a point comes first
    When the agent clicks the text field
    And the agent types "abc"
    And the agent asks what is under the first typed character
    Then the first answer is smaller than the text field

  Scenario: Elements untouched by a change keep their id
    Given the agent remembers the id of the "Save" button
    When the agent clicks the "Save" button
    And the agent clicks the "OK" button
    Then the "Save" button still has the remembered id
