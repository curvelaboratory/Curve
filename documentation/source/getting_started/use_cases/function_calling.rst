.. _curve _function_calling_agentic_guide:

Agentic (Text-to-Action) Apps
==============================

Curve helps you easily personalize your applications by calling application-specific (API) functions
via user prompts. This involves any predefined functions or APIs you want to expose to users to perform tasks,
gather information, or manipulate data. This capability is generally referred to as **function calling**, where
you have the flexibility to support “agentic” apps tailored to specific use cases - from updating insurance
claims to creating ad campaigns - via prompts.

Curve analyzes prompts, extracts critical information from prompts, engages in lightweight conversation with
the user to gather any missing parameters and makes API calls so that you can focus on writing business logic.
Curve does this via its purpose-built :ref:`Curve-FC LLM <llms_in_curve >` - the fastest (200ms p90 - 10x faser than GPT-4o)
and cheapest (100x than GPT-40) function-calling LLM that matches performance with frontier models.
______________________________________________________________________________________________

.. image:: /_static/img/function-calling-network-flow.jpg
   :width: 100%
   :align: center


Single Function Call
--------------------
In the most common scenario, users will request a single action via prompts, and Curve efficiently processes the
request by extracting relevant parameters, validating the input, and calling the designated function or API. Here
is how you would go about enabling this scenario with Curve:

Step 1: Define prompt targets with functions
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

.. literalinclude:: /_config/function-calling-network-agent.yml
    :language: yaml
    :linenos:
    :emphasize-lines: 16-37
    :caption: Define prompt targets that can enable users to engage with API and backened functions of an app

Step 2: Process request parameters in Flask
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Once the prompt targets are configured as above, handling those parameters is

.. literalinclude:: /_include/parameter_handling_flask.py
    :language: python
    :linenos:
    :caption: Flask API example for parameter extraction via HTTP request parameters

Parallel/ Multiple Function Calling
-----------------------------------
In more complex use cases, users may request multiple actions or need multiple APIs/functions to be called
simultaneously or sequentially. With Curve, you can handle these scenarios efficiently using parallel or multiple
function calling. This allows your application to engage in a broader range of interactions, such as updating
different datasets, triggering events across systems, or collecting results from multiple services in one prompt.

Curve-FC1B is built to manage these parallel tasks efficiently, ensuring low latency and high throughput, even
when multiple functions are invoked. It provides two mechanisms to handle these cases:

Step 1: Define Multiple Function Targets
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

When enabling multiple function calling, define the prompt targets in a way that supports multiple functions or
API calls based on the user's prompt. These targets can be triggered in parallel or sequentially, depending on
the user's intent.

Example of Multiple Prompt Targets in YAML:

.. literalinclude:: /_config/function-calling-network-agent.yml
    :language: yaml
    :linenos:
    :emphasize-lines: 16-37
    :caption: Define prompt targets that can enable users to engage with API and backened functions of an app
