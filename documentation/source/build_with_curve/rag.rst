.. _curve _rag_guide:

RAG Application
===============

The following section describes how Curve can help you build faster, smarter and more accurate
Retrieval-Augmented Generation (RAG) applications.

Parameter Extraction for RAG
----------------------------

To build RAG (Retrieval Augmented Generation) applications, you can configure prompt targets with parameters,
enabling Curve to retrieve critical information in a structured way for processing. This approach improves the
retrieval quality and speed of your application. By extracting parameters from the conversation, you can pull
the appropriate chunks from a vector database or SQL-like data store to enhance accuracy. With Curve, you can
streamline data retrieval and processing to build more efficient and precise RAG applications.

Step 1: Define Prompt Targets
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. literalinclude:: includes/rag/prompt_targets.yaml
    :language: yaml
    :caption: Prompt Targets
    :linenos:

Step 2: Process Request Parameters in Flask
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Once the prompt targets are configured as above, handling those parameters is

.. literalinclude:: includes/rag/parameter_handling.py
    :language: python
    :caption: Parameter handling with Flask
    :linenos:

[Coming Soon] `Drift Detection via Curve Intent-Markers <https://github.com/orgs/curvelaboratory/projects/1/views/1?pane=issue&itemId=82697909>`_
-----------------------------------------------------------------------------------------------------------------------------------------
Developers struggle to efficiently handle ``follow-up`` or ``clarification`` questions. Specifically, when users ask for
changes or additions to previous responses their AI applications often generate entirely new responses instead of adjusting
previous ones. Curve offers ``intent tracking`` as a feature so that developers can know when the user has shifted away from a
previous intent so that they can dramatically improve retrieval accuracy, lower overall token cost and  improve the speed of
their responses back to users.

Curve uses its built-in lightweight NLI and embedding models to know if the user has steered away from an active intent.
Curve's intent-drift detection mechanism is based on its :ref:`prompt target <prompt_target>` primtive. Curve tries to match an incoming
prompt to one of the prompt_targets configured in the gateway. Once it detects that the user has moved away from an active
active intent, Curve adds the ``x-curve -intent-marker`` headers to the request before sending it your application servers.

.. literalinclude:: includes/rag/intent_detection.py
    :language: python
    :linenos:
    :lines: 101-157
    :emphasize-lines: 14-25
    :caption: Intent Detection Example


.. Note::

   Curve is (mostly) stateless so that it can scale in an embarrassingly parrallel fashion. So, while Curve offers
   intent-drift detetction, you still have to maintain converational state with intent drift as metadata. The
   following code snippets show how easily you can build and enrich conversational history with Langchain (in Python),
   so that you can use the most relevant prompts for your retrieval and for prompting upstream LLMs.


Step 1: Define ConversationBufferMemory
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. literalinclude:: includes/rag/intent_detection.py
    :language: python
    :linenos:
    :lines: 1-21

Step 2: Update ConversationBufferMemory with Intents
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. literalinclude:: includes/rag/intent_detection.py
    :language: python
    :linenos:
    :lines: 24-64

Step 3: Get Messages based on latest drift
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. literalinclude:: includes/rag/intent_detection.py
    :language: python
    :linenos:
    :lines: 67-80


You can used the last set of messages that match to an intent to prompt an LLM, use it with an vector-DB for
improved retrieval, etc. With Curve and a few lines of code, you can improve the retrieval accuracy, lower overall
token cost and dramatically improve the speed of their responses back to users.
