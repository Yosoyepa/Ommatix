Feature: Acting on the desktop
  As an agent
  I want to click, type and press keys through Ommatix
  So that I can complete tasks and immediately see their effect

  # Resumen (ES): el agente actúa (clic, texto, teclas) y ve el efecto en la misma llamada.

  Background:
    Given the simulated desktop is running
    And the agent perceives the screen

  Scenario: Clicking Save opens a dialog that perception reports
    When the agent clicks the "Save" button
    Then the dialog is open on the desktop
    And the "OK" button is reported at its exact bounds

  Scenario: Confirming the dialog with OK closes it
    Given the agent clicks the "Save" button
    When the agent clicks the "OK" button
    Then the dialog is closed on the desktop
    And the "OK" button is no longer reported

  Scenario: Escape closes the dialog without saving
    Given the agent clicks the "Save" button
    When the agent presses escape
    Then the dialog is closed on the desktop
    And nothing was saved

  Scenario: Typed text reaches the focused field
    When the agent clicks the text field
    And the agent types "Ommatix"
    And the agent presses backspace
    Then the text field contains "Ommati"

  Scenario: A click outside the screen is rejected before it reaches the desktop
    When the agent clicks at 900, 900
    Then the call fails with "outside the screen"
    And the desktop received no input

  Scenario: A click on an element that no longer exists is rejected
    When the agent clicks element "e999"
    Then the call fails with "unknown element"

  Scenario: Acting before perceiving is rejected
    Given a fresh simulated desktop that has not been perceived
    When the agent clicks at 10, 10
    Then the call fails with "perceive first"

  Scenario: An action is never reported as failed once it was performed
    Given the recorded demo session with dry-run input
    And the recorded session has been played to the end
    When the agent types "hola"
    Then the action is reported as performed with an observation error
    And the dry-run log contains 1 action

  Scenario: Control characters are rejected, keys are pressed instead
    When the agent clicks the text field
    And the agent types a line break
    Then the call fails with "control character"
    And the text field contains ""

  Scenario: Dry-run input never reaches the operating system
    Given the recorded demo session with dry-run input
    And the agent perceives the screen
    When the agent clicks at 540, 316
    Then the action is reported as not live
    And the dry-run log contains 1 action
