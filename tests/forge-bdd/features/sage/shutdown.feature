@sage
Feature: Graceful shutdown
  As an operator
  I want the default models to be stopped when sage shuts down
  So that GPU memory is released instead of leaking across restarts

  # This scenario terminates the sage service under test. Because cucumber runs
  # scenarios concurrently, the harness (src/main.rs) runs the rest of the suite
  # first and then this feature on its own, so nothing else races the shutdown.

  Scenario: Default models are gracefully stopped on shutdown
    Given the sage service was started with model teardown enabled
    When the sage service receives a termination signal
    Then switchboard should be asked to stop the default model instance
