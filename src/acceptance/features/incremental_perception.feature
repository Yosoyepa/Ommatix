Feature: Incremental perception
  As an agent operating a desktop
  I want Ommatix to re-analyse only what changed on screen
  So that every perception step stays within a millisecond-scale budget

  # Resumen (ES): Ommatix solo re-analiza las celdas de la cuadrícula que cambiaron.

  Background:
    Given the simulated desktop is running

  Scenario: The first frame is analysed completely
    When the agent perceives the screen
    Then every grid cell is re-analysed
    And 4 elements are added

  Scenario: A static screen costs no detection work
    Given the agent perceives the screen
    When the agent perceives the screen
    Then no grid cell is re-analysed
    And the scene is unchanged

  Scenario: Typing re-analyses only the text field
    Given the agent perceives the screen
    And the agent clicks the text field
    When the agent types "hola"
    Then fewer than 12 grid cells are re-analysed
    And 1 element is added
