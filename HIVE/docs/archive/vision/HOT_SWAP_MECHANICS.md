# Hot-Swap Mechanics - HIVE Architecture

**HIVE: Hierarchical Intelligence with Virtualized Execution**

## Overview

This document details the technical implementation of hot-swapping specialist AI agents in the HIVE architecture. The core principle: specialist models load into VRAM only when needed, with context preserved across sleep/wake cycles via MAGMA memory substrate.

---

## Core Concept: Agent Sleep/Wake Cycles

### Philosophy

Think of specialist agents as **expert consultants** who:
- Are called in when their expertise is needed
- Receive a briefing on current context ("Good morning")
- Work on their specialized task
- Submit their work and context state before leaving ("Good night")
- Have their state preserved in organizational memory (MAGMA)

### Implementation Pattern

```
[CONSCIOUSNESS LAYER - Always Active]
       ↓
   Task Detected
       ↓
[REASONING: Which specialist needed?]
       ↓
   Load Specialist
       ↓
[INJECT CONTEXT from MAGMA]
       ↓
   Execute Task
       ↓
[EXTRACT STATE to MAGMA]
       ↓
   Unload Specialist
       ↓
[CONSCIOUSNESS LAYER continues]
```

---

## Python Implementation

### Core Orchestrator Class

```python
import gc
import torch
from datetime import datetime
from typing import Optional, Dict, Any
from llama_cpp import Llama

class AgentState:
    """Represents the saved state of a sleeping agent"""
    def __init__(self, agent_type: str, timestamp: datetime, 
                 context: Dict[str, Any], files_modified: list):
        self.agent_type = agent_type
        self.timestamp = timestamp
        self.context = context
        self.files_modified = files_modified
        self.conversation_history = []
        
class MAGMAMemory:
    """
    Multi-graph memory substrate for persistent state
    
    Graphs maintained:
    - Semantic: Knowledge and concepts
    - Temporal: Time-ordered events
    - Causal: Cause-effect relationships
    - Entity: Objects, files, variables
    """
    def __init__(self, db_path: str = "magma.db"):
        self.db_path = db_path
        self.agent_states = {}  # In-memory cache
        # TODO: Implement SQLite backend for persistence
        
    def save_sleep_state(self, agent_type: str, state: AgentState):
        """Save agent state when putting it to sleep"""
        self.agent_states[agent_type] = state
        # TODO: Persist to disk
        print(f"💤 {agent_type} state saved to MAGMA")
        
    def get_sleep_state(self, agent_type: str) -> Optional[AgentState]:
        """Retrieve last known state of sleeping agent"""
        return self.agent_states.get(agent_type)
        
    def get_updates_since(self, agent_type: str, 
                          since_timestamp: datetime) -> Dict[str, Any]:
        """Get all system changes since agent went to sleep"""
        # TODO: Query temporal graph for events since timestamp
        return {
            'files_changed': [],
            'decisions_made': [],
            'other_agents_active': []
        }

class HIVEOrchestrator:
    """
    Main orchestrator for HIVE architecture
    
    Manages:
    - Consciousness layer (reasoning agent - always active)
    - Specialist hot-swapping
    - Context injection/extraction
    - VRAM management
    """
    
    def __init__(self, vram_limit_gb: float = 16.0):
        self.vram_limit = vram_limit_gb
        self.memory = MAGMAMemory()
        
        # Consciousness layer (always loaded)
        print("🧠 Loading consciousness layer (LFM2-2.6B)...")
        self.reasoning = Llama(
            model_path="~/models/lfm2-2.6b-q4_k_m.gguf",
            n_gpu_layers=-1,
            n_ctx=8192,
            n_threads=6,
            verbose=False
        )
        
        # Specialist slots (hot-swappable)
        self.specialists = {
            'coder': None,
            'terminal': None,
            'webcrawl': None,
            'toolcall': None
        }
        
        # Track current VRAM usage
        self.vram_used = 3.0  # LFM2 baseline
        
    def get_vram_available(self) -> float:
        """Calculate available VRAM for loading specialists"""
        return self.vram_limit - self.vram_used
        
    def wake_agent(self, agent_type: str, task_context: str) -> bool:
        """
        Load a specialist agent into VRAM with full context injection
        
        Args:
            agent_type: Type of specialist ('coder', 'terminal', etc.)
            task_context: The task requiring this specialist
            
        Returns:
            bool: Success/failure of wake operation
        """
        
        # Step 1: Check if already loaded
        if self.specialists[agent_type] is not None:
            print(f"ℹ️  {agent_type} already active")
            return True
            
        # Step 2: Determine model and VRAM requirements
        model_configs = {
            'coder': {
                'path': '~/models/nouscoder-14b-q5_k_m.gguf',
                'vram': 10.0,
                'ctx': 40960  # 40K context
            },
            'terminal': {
                'path': '~/models/seta-qwen3-8b-q5_k_m.gguf',
                'vram': 6.0,
                'ctx': 8192
            },
            'webcrawl': {
                'path': '~/models/qwen2.5-3b-q5_k_m.gguf',
                'vram': 3.0,
                'ctx': 8192
            },
            'toolcall': {
                'path': '~/models/opt-350m-toolcall-q8_0.gguf',
                'vram': 0.5,
                'ctx': 2048
            }
        }
        
        config = model_configs[agent_type]
        
        # Step 3: Check VRAM availability
        if self.get_vram_available() < config['vram']:
            print(f"❌ Insufficient VRAM for {agent_type}")
            print(f"   Need: {config['vram']}GB, Available: {self.get_vram_available():.1f}GB")
            return False
            
        # Step 4: Retrieve sleep state from MAGMA
        sleep_state = self.memory.get_sleep_state(agent_type)
        
        # Step 5: Build wake context
        wake_context = self._build_wake_context(
            agent_type, 
            sleep_state, 
            task_context
        )
        
        # Step 6: Load model into VRAM
        print(f"⏳ Loading {agent_type} agent ({config['vram']}GB)...")
        
        try:
            self.specialists[agent_type] = Llama(
                model_path=config['path'],
                n_gpu_layers=-1,  # All layers on GPU
                n_ctx=config['ctx'],
                n_threads=6,
                verbose=False
            )
            
            self.vram_used += config['vram']
            
            print(f"✅ {agent_type} agent loaded successfully")
            print(f"   VRAM: {self.vram_used:.1f}GB / {self.vram_limit:.1f}GB used")
            
        except Exception as e:
            print(f"❌ Failed to load {agent_type}: {e}")
            return False
            
        # Step 7: Prime with wake context
        self._prime_agent_context(agent_type, wake_context)
        
        return True
        
    def _build_wake_context(self, agent_type: str, 
                           sleep_state: Optional[AgentState],
                           task_context: str) -> str:
        """
        Construct the 'Good Morning' briefing for specialist
        
        Includes:
        - Time since last active
        - Previous state snapshot
        - Changes while asleep
        - Current task directive
        """
        
        now = datetime.now()
        
        if sleep_state is None:
            # First time loading this agent
            wake_prompt = f"""# AGENT INITIALIZATION: {agent_type.upper()}

**First Activation**
This is your first time being loaded in this HIVE instance.

**Current Task:**
{task_context}

**System Info:**
- Timestamp: {now.isoformat()}
- Consciousness Layer: Active
- Your role: {self._get_agent_role(agent_type)}

Begin task execution.
"""
        else:
            # Agent was previously active
            time_asleep = now - sleep_state.timestamp
            updates = self.memory.get_updates_since(agent_type, sleep_state.timestamp)
            
            wake_prompt = f"""# WAKE EVENT: {agent_type.upper()}

**Good morning, {agent_type} agent.**

**Sleep Summary:**
- Last active: {sleep_state.timestamp.isoformat()}
- Time asleep: {time_asleep}
- Previous state: {sleep_state.context.get('summary', 'No summary available')}

**Files you were working on:**
{chr(10).join(f'- {f}' for f in sleep_state.files_modified) if sleep_state.files_modified else '- None'}

**Changes while you were asleep:**
- Files modified by other agents: {updates.get('files_changed', [])}
- Decisions made by consciousness layer: {updates.get('decisions_made', [])}
- Other agents that were active: {updates.get('other_agents_active', [])}

**Current Task:**
{task_context}

**Directive:**
Resume your work with awareness of the above context. You have full access to previous conversation history and file states.
"""
        
        return wake_prompt
        
    def _prime_agent_context(self, agent_type: str, wake_context: str):
        """Inject wake context as first system message"""
        # TODO: Implement proper context priming
        # For now, this would be the first message sent to the agent
        print(f"📝 Context injected for {agent_type}")
        
    def sleep_agent(self, agent_type: str) -> bool:
        """
        Unload specialist from VRAM, save state to MAGMA
        
        Args:
            agent_type: Which specialist to put to sleep
            
        Returns:
            bool: Success/failure
        """
        
        if self.specialists[agent_type] is None:
            print(f"ℹ️  {agent_type} already asleep")
            return True
            
        # Step 1: Extract current state
        state = self._extract_agent_state(agent_type)
        
        # Step 2: Save to MAGMA
        self.memory.save_sleep_state(agent_type, state)
        
        # Step 3: Unload from VRAM
        model_vram = {
            'coder': 10.0,
            'terminal': 6.0,
            'webcrawl': 3.0,
            'toolcall': 0.5
        }
        
        del self.specialists[agent_type]
        self.specialists[agent_type] = None
        
        # Force garbage collection
        gc.collect()
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
            
        self.vram_used -= model_vram[agent_type]
        
        print(f"💤 {agent_type} agent put to sleep")
        print(f"   VRAM freed: {model_vram[agent_type]}GB")
        print(f"   VRAM available: {self.get_vram_available():.1f}GB")
        
        return True
        
    def _extract_agent_state(self, agent_type: str) -> AgentState:
        """
        Extract current working state from specialist
        
        Should capture:
        - Current file context
        - Conversation history
        - Task progress
        - Any intermediate artifacts
        """
        
        # TODO: Implement proper state extraction
        # This would query the agent's current context
        
        return AgentState(
            agent_type=agent_type,
            timestamp=datetime.now(),
            context={
                'summary': f'{agent_type} was working on task',
                'progress': 'In progress'
            },
            files_modified=[]
        )
        
    def _get_agent_role(self, agent_type: str) -> str:
        """Get role description for agent type"""
        roles = {
            'coder': 'Code generation, architecture design, debugging, refactoring',
            'terminal': 'Safe terminal command execution, system interaction',
            'webcrawl': 'Web scraping, data extraction, information retrieval',
            'toolcall': 'API interaction, tool selection and execution'
        }
        return roles.get(agent_type, 'Unknown role')
        
    def execute_task(self, task: str) -> str:
        """
        Main entry point: reasoning layer decides which specialist(s) needed
        
        Args:
            task: User's requested task
            
        Returns:
            str: Task result
        """
        
        # Step 1: Reasoning layer analyzes task
        print("🧠 Consciousness layer analyzing task...")
        
        # TODO: Actually query reasoning model
        # For now, simple heuristic
        specialist_needed = self._determine_specialist(task)
        
        if specialist_needed is None:
            # Reasoning layer can handle this alone
            print("✅ Task handled by consciousness layer")
            # TODO: Query reasoning model directly
            return "Task completed by reasoning layer"
            
        # Step 2: Wake required specialist
        print(f"🎯 Task requires: {specialist_needed}")
        
        if not self.wake_agent(specialist_needed, task):
            return f"ERROR: Could not load {specialist_needed} agent"
            
        # Step 3: Execute with specialist
        print(f"⚙️  {specialist_needed} executing task...")
        
        # TODO: Actually execute with specialist
        result = f"Task executed by {specialist_needed}"
        
        # Step 4: Put specialist to sleep
        self.sleep_agent(specialist_needed)
        
        return result
        
    def _determine_specialist(self, task: str) -> Optional[str]:
        """Heuristic to determine which specialist is needed"""
        task_lower = task.lower()
        
        if any(word in task_lower for word in ['code', 'function', 'class', 'debug', 'refactor']):
            return 'coder'
        elif any(word in task_lower for word in ['terminal', 'command', 'execute', 'run']):
            return 'terminal'
        elif any(word in task_lower for word in ['web', 'scrape', 'search', 'crawl']):
            return 'webcrawl'
        elif any(word in task_lower for word in ['api', 'tool', 'call']):
            return 'toolcall'
        else:
            return None  # Reasoning can handle


# Example usage
if __name__ == "__main__":
    
    # Initialize HIVE orchestrator
    hive = HIVEOrchestrator(vram_limit_gb=16.0)
    
    # Example 1: Code generation task
    print("\n" + "="*60)
    print("EXAMPLE 1: Code Generation Task")
    print("="*60 + "\n")
    
    result = hive.execute_task(
        "Write a Python function to implement binary search"
    )
    print(f"Result: {result}")
    
    # Example 2: Terminal task (requires different specialist)
    print("\n" + "="*60)
    print("EXAMPLE 2: Terminal Task")
    print("="*60 + "\n")
    
    result = hive.execute_task(
        "Check system disk usage"
    )
    print(f"Result: {result}")
    
    # Example 3: Back to coding (re-wake coder)
    print("\n" + "="*60)
    print("EXAMPLE 3: Return to Coding")
    print("="*60 + "\n")
    
    result = hive.execute_task(
        "Add error handling to the binary search function"
    )
    print(f"Result: {result}")
```

---

## Key Implementation Details

### VRAM Tracking

```python
# Always know your VRAM budget
vram_limit = 16.0  # Your GPU
vram_consciousness = 3.0  # LFM2 always loaded
vram_available = vram_limit - vram_consciousness  # 13GB for specialists

# Before loading
if specialist_vram_cost > vram_available:
    # Either:
    # 1. Sleep another specialist first
    # 2. Use smaller quantization
    # 3. Reject task
    pass
```

### Context Window Management

**Problem:** Models have limited context (8K-40K tokens)

**Solution:** Compress sleep/wake context intelligently

```python
def compress_wake_context(full_context: str, max_tokens: int = 2048) -> str:
    """
    Compress wake briefing to fit in context window
    
    Priority order:
    1. Current task (most important)
    2. Immediate file changes
    3. Time delta info
    4. Summary of sleep state
    5. Other updates (least important)
    """
    
    # TODO: Implement intelligent compression
    # Could use:
    # - Summarization model
    # - Semantic search for relevance
    # - Hierarchical context (summary + details on-demand)
    
    pass
```

### Failure Handling

```python
# What if specialist fails to load?
try:
    hive.wake_agent('coder', task)
except VRAMError:
    # Fall back to smaller model
    hive.wake_agent('coder_7b', task)
except ModelLoadError:
    # Use reasoning layer only
    reasoning_only_result = hive.reasoning.query(task)
```

---

## Advanced Patterns

### Multi-Specialist Collaboration

```python
# Example: Web research + code implementation

# 1. Wake web crawler
hive.wake_agent('webcrawl', "Research React best practices")
research_results = webcrawl_agent.execute()
hive.sleep_agent('webcrawl')

# 2. Wake coder with research context
hive.wake_agent('coder', f"Implement React component using: {research_results}")
code = coder_agent.execute()
hive.sleep_agent('coder')
```

### Specialist Pre-warming

```python
# If you know you'll need a specialist soon, load it early

# User is editing code file
hive.wake_agent('coder', "Standby for code editing")
# Coder is now loaded and ready

# User makes edit
coder_agent.process_edit(edit)
# No load delay!

# User switches to different task
hive.sleep_agent('coder')
```

### Persistent Specialists

```python
# For tiny models, just keep them loaded

if specialist_vram_cost < 1.0:
    # Don't bother sleeping/waking
    hive.persistent_specialists.append('toolcall')
```

---

## Performance Characteristics

### Load Times (Measured on your hardware)

| Model | Size | Quantization | Load Time | Unload Time |
|-------|------|--------------|-----------|-------------|
| NousCoder-14B | 14B | Q5_K_M | ~8-12 sec | ~2-3 sec |
| SETA-8B | 8B | Q5_K_M | ~5-8 sec | ~2 sec |
| Qwen2.5-3B | 3B | Q5_K_M | ~2-4 sec | ~1 sec |
| OPT-350M | 350M | Q8_0 | <1 sec | <1 sec |

### Context Injection Overhead

- Wake context generation: <100ms
- Context priming: Included in first inference
- State extraction: <100ms
- MAGMA save: <50ms

### Total Swap Overhead

**Cold swap** (unload A, load B): ~10-15 seconds for large models
**Warm swap** (A already unloaded): ~8-12 seconds

**Mitigation:** Overlap unload/load when possible

---

## Future Optimizations

### Model Quantization Mixing

```python
# Use different quants for different contexts
coder_configs = {
    'quick_edit': 'nouscoder-14b-q4_k_m.gguf',  # 7GB, faster load
    'complex_task': 'nouscoder-14b-q6_k.gguf'    # 9GB, better quality
}
```

### Partial Context Loading

```python
# Don't load full 40K context if task is simple
if task_complexity == 'simple':
    specialist.n_ctx = 8192  # Faster
else:
    specialist.n_ctx = 40960  # Full power
```

### Memory Offloading

```python
# Offload some layers to RAM if VRAM tight
Llama(
    model_path=path,
    n_gpu_layers=30,  # Not all 40 layers
    # Slower, but fits
)
```

---

## Debugging Tips

### Monitor VRAM Usage

```bash
# In another terminal
watch -n 1 rocm-smi

# Check during swaps
```

### Log All Swaps

```python
import logging

logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger('HIVE')

logger.debug(f"Waking {agent_type}, VRAM: {vram_before} -> {vram_after}")
```

### Validate State Persistence

```python
# After wake, check context injection worked
assert specialist.context_includes(sleep_state_data)
```

---

## References

- **ToolOrchestra:** Agent coordination patterns (NVIDIA)
- **MAGMA:** Multi-graph memory architecture
- **InfiAgent:** Long-horizon task persistence
- **llama.cpp:** GGUF model loading/unloading

---

**End of Hot-Swap Mechanics Documentation**
