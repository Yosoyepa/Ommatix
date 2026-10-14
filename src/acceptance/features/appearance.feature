Feature: Perception that does not depend on the theme
  As an agent working on someone else's desktop
  I want elements found the same way on light, dark and contrasting desktops
  So that I never have to tune Ommatix to a colour scheme

  # Resumen (ES): el detector no asume un color de fondo: funciona con tema claro,
  # oscuro y con un fondo de escritorio que contrasta con la ventana.

  Scenario Outline: The form is found whatever the theme
    Given the simulated desktop is running with the <theme> theme
    When the agent perceives the screen
    Then <count> elements are added
    And the "Save" button is reported at its exact bounds
    And the text field is reported at its exact bounds

    Examples:
      | theme                 | count |
      | light                 | 4     |
      | dark                  | 4     |
      | contrasting wallpaper | 5     |

  Scenario: A window on a contrasting wallpaper is an element, the wallpaper is not
    Given the simulated desktop is running with the contrasting wallpaper theme
    When the agent perceives the screen
    Then the window is reported at its exact bounds
    And the title bar is reported at its exact bounds

  Scenario: Typing inside a window does not re-analyse the window
    Given the simulated desktop is running with the contrasting wallpaper theme
    And the agent perceives the screen
    And the agent remembers the id of the window
    And the agent clicks the text field
    When the agent types "hola"
    Then at most 5% of the screen is re-analysed
    And the window was not re-analysed
    And the window still has the remembered id

  Scenario: Light text on a dark theme is grouped into one element
    Given the simulated desktop is running with the dark theme
    And the agent perceives the screen
    And the agent clicks the text field
    When the agent types "hola"
    Then the typed text "hola" is reported as one element
