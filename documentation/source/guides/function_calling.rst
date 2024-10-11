.. _function_calling:

Function Calling
================

**Function Calling** is a powerful feature in Curve that allows your application to dynamically execute backend functions or services based on user prompts.
This enables seamless integration between natural language interactions and backend operations, turning user inputs into actionable results.


What is Function Calling?
-------------------------

Function Calling refers to the mechanism where the user's prompt is parsed, relevant parameters are extracted, and a designated backend function (or API) is triggered to execute a particular task.
This feature bridges the gap between generative AI systems and functional business logic, allowing users to interact with the system through natural language while the backend performs the necessary operations.

Function Calling Workflow
-------------------------

#. **Prompt Parsing**

    When a user submits a prompt, Curve analyzes it to determine the intent. Based on this intent, the system identifies whether a function needs to be invoked and which parameters should be extracted.

#. **Parameter Extraction**

    Curve’s advanced natural language processing capabilities automatically extract parameters from the prompt that are necessary for executing the function. These parameters can include text, numbers, dates, locations, or other relevant data points.

#. **Function Invocation**

    Once the necessary parameters have been extracted, Curve invokes the relevant backend function. This function could be an API, a database query, or any other form of backend logic. The function is executed with the extracted parameters to produce the desired output.

#. **Response Handling**

    After the function has been called and executed, the result is processed and a response is generated. This response is typically delivered in a user-friendly format, which can include text explanations, data summaries, or even a confirmation message for critical actions.


Curve-Function
-------------------------
The `Curve-Function <https://huggingface.co/collections/curvelaboratory/Curve-function-66f209a693ea8df14317ad68>`_ collection of large language models (LLMs) is a collection state-of-the-art (SOTA) LLMs specifically designed for **function calling** tasks.
The models are designed to understand complex function signatures, identify required parameters, and produce accurate function call outputs based on natural language prompts.
Achieving performance on par with GPT-4, these models set a new benchmark in the domain of function-oriented tasks, making them suitable for scenarios where automated API interaction and function execution is crucial.

In summary, the Curve-Function collection demonstrates:

- **State-of-the-art performance** in function calling
- **Accurate parameter identification and suggestion**, even in ambiguous or incomplete inputs
- **High generalization** across multiple function calling use cases, from API interactions to automated backend tasks.
- Optimized **low-latency, high-throughput performance**, making it suitable for real-time, production environments.


Key Features
~~~~~~~~~~~~
.. table::
    :width: 100%

    =========================   ===============================================================
    **Functionality**	        **Definition**
    =========================   ===============================================================
    Single Function Calling	    Call only one function per user prompt
    Parallel Function Calling	Call the same function multiple times but with parameter values
    Multiple Function Calling	Call different functions per user prompt
    Parallel & Multiple	        Perform both parallel and multiple function calling
    =========================   ===============================================================


Supported Languages
~~~~~~~~~~~~~~~~~~~
.. table::
    :width: 100%

    =========================   ===========================================================================================================================================
    **Language**	            **Data Type**
    =========================   ===========================================================================================================================================
    Python	                    ``int``, ``str``, ``float``, ``bool``, ``list``, ``set``, ``dict``, ``tuple``
    Java	                    ``byte``, ``short``, ``int``, ``long``, ``float``, ``double``, ``boolean``, ``char``, ``Array``, ``ArrayList``, ``Set``, ``HashMap``, ``Hashtable``, ``Queue``, ``Stack``
    Javascript	                ``Number``, ``Bigint``, ``String``, ``Boolean``, ``Object``, ``Array``, ``Date``
    =========================   ===========================================================================================================================================


Implementing Function Calling
-----------------------------

Here’s a step-by-step guide to configuring function calling within your Curve setup:

Step 1: Define the Function
~~~~~~~~~~~~~~~~~~~~~~~~~~~
First, create or identify the backend function you want Curve to call. This could be an API endpoint, a script, or any other executable backend logic.

.. code-block:: python
    :caption: Example Function

    import requests

    def get_weather(location: str, unit: str = "fahrenheit"):
        if unit not in ["celsius", "fahrenheit"]:
            raise ValueError("Invalid unit. Choose either 'celsius' or 'fahrenheit'.")

        api_server = "https://api.yourweatherapp.com"
        endpoint = f"{api_server}/weather"

        params = {
            "location": location,
            "unit": unit
        }

        response = requests.get(endpoint, params=params)
        return response.json()

    # Example usage
    weather_info = get_weather("Seattle, WA", "celsius")
    print(weather_info)


Step 2: Configure Prompt Targets
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Next, map the function to a prompt target, defining the intent and parameters that Curve will extract from the user’s prompt.
Specify the parameters your function needs and how Curve should interpret these.

.. code-block:: yaml
    :caption: Prompt Target Example Configuration

    prompt_targets:
      - name: get_weather
        description: Get the current weather for a location
        parameters:
          - name: location
            description: The city and state, e.g. San Francisco, New York
            type: str
            required: true
          - name: unit
            description: The unit of temperature to return
            type: str
            enum: ["celsius", "fahrenheit"]
        endpoint:
          name: api_server
          path: /weather

Step 3: Curve Takes Over
~~~~~~~~~~~~~~~~~~~~~~~
Once you have defined the functions and configured the prompt targets, Curve takes care of the remaining work.
It will automatically validate parameters validate parameters and ensure that the required parameters (e.g., location) are present in the prompt, and add validation rules if necessary.
Here is ane example validation schema using the `jsonschema <https://json-schema.org/docs>`_ library

.. code-block:: python
    :caption: Example Validation Schema

    import requests
    from jsonschema import validate, ValidationError

    # Define the JSON Schema for parameter validation
    weather_validation_schema = {
        "type": "object",
        "properties": {
            "location": {
                "type": "string",
                "minLength": 1,
                "description": "The city and state, e.g. 'San Francisco, New York'"
            },
            "unit": {
                "type": "string",
                "enum": ["celsius", "fahrenheit"],
                "description": "The unit of temperature to return"
            }
        },
        "required": ["location"],
        "additionalProperties": False
    }

    def get_weather(location: str, unit: str = "fahrenheit"):
        # Create the data object for validation
        params = {
            "location": location,
            "unit": unit
        }

        # Validate parameters using JSON Schema
        try:
            validate(instance=params, schema=weather_validation_schema)
        except ValidationError as e:
            raise ValueError(f"Invalid input: {e.message}")

        # Prepare the API request
        api_server = "https://api.yourweatherapp.com"
        endpoint = f"{api_server}/weather"

        # Make the API request
        response = requests.get(endpoint, params=params)
        return response.json()

    # Example usage
    weather_info = get_weather("Seattle, WA", "celsius")
    print(weather_info)


Once the functions are called, Curve formats the response and deliver back to users.
By completing these setup steps, you enable Curve to manage the process from validation to response, ensuring users receive consistent, reliable results.

Example Use Cases
-----------------

Here are some common use cases where Function Calling can be highly beneficial:

- **Data Retrieval**: Extracting information from databases or APIs based on user inputs (e.g., checking account balances, retrieving order status).
- **Transactional Operations**: Executing business logic such as placing an order, processing payments, or updating user profiles.
- **Information Aggregation**: Fetching and combining data from multiple sources (e.g., displaying travel itineraries or combining analytics from various dashboards).
- **Task Automation**: Automating routine tasks like setting reminders, scheduling meetings, or sending emails.
- **User Personalization**: Tailoring responses based on user history, preferences, or ongoing interactions.

Best Practices and Tips
-----------------------
When integrating function calling into your generative AI applications, keep these tips in mind to get the most out of our Curve-Function models:

- **Keep it clear and simple**: Your function names and parameters should be straightforward and easy to understand. Think of it like explaining a task to a smart colleague - the clearer you are, the better the results.

- **Context is king**: Don't skimp on the descriptions for your functions and parameters. The more context you provide, the better the LLM can understand when and how to use each function.

- **Be specific with your parameters**: Instead of using generic types, get specific. If you're asking for a date, say it's a date. If you need a number between 1 and 10, spell that out. The more precise you are, the more accurate the LLM's responses will be.

- **Expect the unexpected**: Test your functions thoroughly, including edge cases. LLMs can be creative in their interpretations, so it's crucial to ensure your setup is robust and can handle unexpected inputs.

- **Watch and learn**: Pay attention to how the LLM uses your functions. Which ones does it call often? In what contexts? This information can help you optimize your setup over time.

Remember, working with LLMs is part science, part art. Don't be afraid to experiment and iterate to find what works best for your specific use case.
